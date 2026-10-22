# Abhiverse coordination API — multi-stage build.
FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p abhiverse-api

FROM debian:bookworm-slim
RUN useradd --system --create-home abhiverse
COPY --from=builder /src/target/release/abhiverse-api /usr/local/bin/abhiverse-api
ENV ABHIVERSE_BIND_ADDR=0.0.0.0:8080 \
    ABHIVERSE_STATE_FILE=/data/state.json \
    ABHIVERSE_TRANSFER_DIR=/data/chunks
VOLUME /data
USER abhiverse
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s \
  CMD bash -c 'echo > /dev/tcp/127.0.0.1/8080' || exit 1
ENTRYPOINT ["abhiverse-api"]
