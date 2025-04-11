use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio_tungstenite::connect_async;
use tungstenite::protocol::Message;
use uuid::Uuid;

type PendingWebSocketRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;
type NotificationQueue = Arc<Mutex<Vec<Value>>>;

pub struct JsonRpcClient {
    /// Sender for outgoing WebSocket requests
    sender: mpsc::UnboundedSender<Message>,
    /// Shared map for pending RPC calls
    pending_requests: PendingWebSocketRequests,
    /// Stores incoming notifications
    notifications: NotificationQueue,
    /// Notify listeners of new notifications
    notify: Arc<Notify>,
}

impl JsonRpcClient {
    pub async fn new(url: &str) -> Self {
        let (ws_stream, _) = connect_async(url)
            .await
            .expect("Failed to connect to server.");

        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel();

        let pending_requests: PendingWebSocketRequests = Arc::new(Mutex::new(HashMap::new()));
        let notifications: NotificationQueue = Arc::new(Mutex::new(Vec::new()));
        let notify: Arc<Notify> = Arc::new(Notify::new());

        // Spawn task that forwards requests to the WebSocket server
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = write.send(msg).await {
                    eprintln!("Error sending message via websocket: {}", e);
                    break;
                }
            }
        });

        let pending_read_requests: PendingWebSocketRequests = pending_requests.clone();
        let notifications_clone = notifications.clone();
        let notify_clone = notify.clone();

        // Spawn tasks that reads responses and notifications from the WebSocket server
        tokio::spawn(async move {
            while let Some(response) = read.next().await {
                let msg = match response {
                    Ok(msg) => msg,
                    Err(e) => {
                        eprintln!("Error reading message: {}", e);
                        continue;
                    }
                };

                let text = match msg {
                    Message::Text(text) => text,
                    _ => continue, // ignore non-text messages
                };

                let value: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(val) => {
                        log::info!("WSS received: {}", val);
                        val
                    }
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} in text: {}", e, text);
                        continue;
                    }
                };

                match value.get("id").and_then(|v| v.as_str()) {
                    // check if ID exists. if so, pass as response, else pass as notification
                    Some(id) => {
                        let mut pending_requests = pending_read_requests.lock().await;
                        if let Some(sender) = pending_requests.remove(id) {
                            if sender.send(value.clone()).is_err() {
                                eprintln!("Warning: receiver for id {} dropped", id);
                            }
                        }
                    }
                    None => {
                        let mut queue = notifications_clone.lock().await;
                        queue.push(value);
                        notify_clone.notify_waiters();
                    }
                }
            }
        });

        Self {
            sender: tx,
            pending_requests,
            notifications,
            notify,
        }
    }

    pub async fn call_method(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, anyhow::Error> {
        let id = Uuid::new_v4().to_string();
        let request = json!({
            "id": id,
            "method": method,
            "params": params
        });

        let msg = Message::Text(request.to_string().into());

        let (resp_tx, resp_rx) = oneshot::channel();
        self.pending_requests.lock().await.insert(id, resp_tx);
        self.sender.send(msg)?;

        let response = resp_rx.await?;
        Ok(response)
    }

    pub async fn wait_for_notification(&self) -> Value {
        loop {
            {
                let mut queue = self.notifications.lock().await;
                if let Some(notif) = queue.pop() {
                    return notif;
                }
            }
            self.notify.notified().await;
        }
    }
}
