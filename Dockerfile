FROM rust:latest AS builder

WORKDIR /usr/src/mooze-dealer

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev \
    build-essential cmake && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY . .

FROM debian:latest

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/mooze-dealer/target/release/mooze-dealer /app/
COPY config.toml log4rs.yaml /app/

RUN mkdir -p /app/logs

EXPOSE 8080

CMD ["./mooze-dealer"]
