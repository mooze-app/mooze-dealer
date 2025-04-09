# ---------- Builder stage ----------
FROM rust:latest AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev \
    build-essential cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/mooze-dealer

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY . .
RUN cargo build --release

# ---------- Runtime stage ----------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/mooze-dealer/target/release/mooze-dealer /app/
COPY config.toml log4rs.yaml /app/

RUN chmod +x /app/mooze-dealer
RUN mkdir -p /app/logs

EXPOSE 8080

CMD ["./mooze-dealer"]