FROM rust:bookworm AS builder

WORKDIR /app
COPY . .

RUN cargo build --release --locked

FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /app/target/release/linguaspark-server /app/linguaspark-server

ENV MODELS_DIR=/app/models
ENV NUM_WORKERS=
ENV IP=0.0.0.0
ENV PORT=3000
# ENV ENV_API_KEY=
ENV RUST_LOG=info

EXPOSE 3000

ENTRYPOINT ["/app/linguaspark-server"]
