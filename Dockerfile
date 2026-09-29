# Multi-stage build for ReDash Web Server & Gateway
FROM rust:1.96-slim AS builder

WORKDIR /usr/src/redash
COPY . .

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.128 --locked
RUN bash scripts/build_web.sh && cargo build --locked --release --bin redash-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates openssh-client && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/redash/target/release/redash-server /usr/local/bin/redash-server

EXPOSE 8080
ENV PORT=8080
ENV HOST=0.0.0.0

CMD ["redash-server", "--host", "0.0.0.0", "--port", "8080"]
