# Multi-stage build for ultra-lightweight ReDash Agent (< 15MB)
FROM alpine:3.20 as runtime

RUN apk add --no-cache ca-certificates tzdata curl procps iproute2

# Install pre-built static binary or local binary
ARG TARGETARCH=amd64
COPY target/release/redash-agent /usr/local/bin/redash-agent

ENV REDASH_HUB_URL="ws://127.0.0.1:8080/v1/agent/ws" \
    REDASH_NODE_ID="" \
    REDASH_AUTH_TOKEN="default-token" \
    REDASH_TRUSTED_KEY=""

VOLUME ["/var/run/docker.sock"]

ENTRYPOINT ["/usr/local/bin/redash-agent"]
