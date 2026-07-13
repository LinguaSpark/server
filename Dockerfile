FROM rust:bookworm AS builder

WORKDIR /app
COPY . .

RUN cargo build --release

RUN mkdir -p /app/lib && \
    find /app/target/release/build -name "linguaspark-*" -type d | xargs -I {} find {} -path "*/out/*.so" -type f | xargs -I {} cp {} /app/lib/ && \
    ls -l /app/lib

FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /app/target/release/linguaspark-server /app/linguaspark-server
COPY --from=builder /app/lib/*.so /usr/local/lib/
RUN ldconfig

ENV MODELS_DIR=/app/models
ENV NUM_WORKERS=1
ENV IP=0.0.0.0
ENV PORT=3000
# ENV ENV_API_KEY=
ENV RUST_LOG=info

EXPOSE 3000

ENTRYPOINT ["/app/linguaspark-server"]
