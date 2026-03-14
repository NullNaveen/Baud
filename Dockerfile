# syntax=docker/dockerfile:1

# --- Build stage ---
FROM rust:1.83-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary
RUN cargo build --release --bin baud-node

# --- Runtime stage ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash baud
USER baud
WORKDIR /home/baud

COPY --from=builder /build/target/release/baud-node /usr/local/bin/baud-node

EXPOSE 9090 9091

VOLUME ["/home/baud/data"]

ENTRYPOINT ["baud-node"]
CMD ["--bind", "0.0.0.0:9090", "--data-dir", "/home/baud/data"]
