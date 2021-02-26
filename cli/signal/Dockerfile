FROM rust:latest as builder
WORKDIR /usr/src/signal
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
# RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/signal /usr/local/bin/signal
CMD ["signal"]
