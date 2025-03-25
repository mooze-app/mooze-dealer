# Mooze Dealer

A Rust-based service that facilitates cryptocurrency transactions with PIX (Brazilian instant payment system) integration. The application enables users to deposit funds via PIX and receive cryptocurrency assets like DEPIX, USDT, and LBTC on the Liquid Network.

## Features

- **User Management**: Create and verify user accounts with referral capabilities
- **PIX Integration**: Process deposits through the Brazilian PIX payment system
- **Liquid Network Support**: Handle transactions on the Liquid Network
- **Transaction Processing**: Manage and process cryptocurrency transactions
- **Fee Management**: Calculate and collect fees for transactions
- **RESTful API**: Interact with the service via HTTP endpoints
- **WebSocket JSON-RPC Client**: Communicate with other services via WebSockets

## System Architecture

### Core Components

- **Services**: Independent, message-based components handling specific business logic
  - Transaction Service
  - PIX Service
  - Liquid Service
  - User Service
  - HTTP Service

- **Repositories**: Data access layer for external services and database
  - Transaction Repository
  - PIX Repository
  - Liquid Repository
  - User Repository

- **Models**: Data structures representing core entities
  - Transaction
  - PIX Deposit
  - User
  - Referral
  - Assets

### Technologies

- **Rust**: Core programming language
- **Tokio**: Asynchronous runtime
- **SQLx**: Database access (PostgreSQL)
- **Axum**: HTTP server framework
- **Liquid Wallet Kit (lwk_wollet)**: Liquid Network integration
- **WebSockets**: For service-to-service communication

## Prerequisites

- Rust 1.65+
- PostgreSQL 12+
- Access to Liquid Network (via Electrum server)
- PIX API credentials (Eulen)

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/mooze-dealer.git
   cd mooze-dealer
   ```

2. Create a `config.toml` file in the project root:
   ```toml
   [postgres]
   url = "postgres://username:password@localhost/mooze"
   port = 5432
   user = "username"
   password = "password"
   database = "mooze"

   [electrum]
   url = "electrum.server.address"
   port = 50001
   tls = true
   testnet = false

   [depix]
   url = "https://api.depix.service"
   auth_token = "your_depix_auth_token"
   tls = true

   [sideswap]
   url = "https://sideswap.api.address"

   [wallet]
   mnemonic = "your wallet mnemonic seed phrase here"
   mainnet = true
   ```

3. Set up the database schema (create a migration script based on the models)

4. Build the application:
   ```bash
   cargo build --release
   ```

## Running the Application

Execute the compiled binary:

```bash
./target/release/mooze-dealer
```

The application will start all services and listen for HTTP requests on port 8080 by default.

## API Endpoints

### User Management

- **POST /user**: Create a new user
  ```json
  {
    "referral_code": "optional_referral_code"
  }
  ```

### Deposits

- **POST /deposit**: Request a new deposit
  ```json
  {
    "user_id": "user_uuid",
    "address": "destination_address",
    "amount_in_cents": 10000,
    "asset": "asset_id_or_symbol",
    "network": "liquid"
  }
  ```

### PIX Integration

- **POST /eulen_update_status**: Update PIX deposit status (webhook)
  ```json
  {
    "bank_tx_id": "bank_transaction_id",
    "blockchain_tx_id": "blockchain_transaction_id",
    "customer_message": "message",
    "payer_name": "Payer Name",
    "payer_tax_number": "tax_id",
    "expiration": "expiration_date",
    "pix_key": "pix_key",
    "qr_id": "qr_code_id",
    "status": "completed",
    "value_in_cents": 10000
  }
  ```

### Health Check

- **GET /health**: Check service health
- **GET /hello**: Simple hello endpoint

## Development

### Project Structure

```
src/
├── main.rs            # Application entry point
├── models/            # Data structures
├── repositories/      # Data access layer
├── services/          # Business logic
├── settings.rs        # Configuration
└── utils/             # Utility functions
```

### Building and Testing

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run with development settings
cargo run
```

## License

[GNU General Public License v3.0 (GPL-3.0)](LICENSE)

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
