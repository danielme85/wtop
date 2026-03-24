# Stage 1: Build
FROM rust:1-bookworm AS builder
WORKDIR /usr/src/wtop
COPY Cargo.toml Cargo.lock* ./
COPY build.rs ./
COPY src/ src/
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/wtop/target/release/wtop /usr/local/bin/wtop
ENTRYPOINT ["wtop"]
