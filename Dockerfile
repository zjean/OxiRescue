FROM rust:1.93-bookworm AS builder

WORKDIR /build

# Cache dependency builds: copy manifests first, build a dummy, then overlay real source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && touch src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY src/ src/
RUN touch src/main.rs src/lib.rs && cargo build --release

# -------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libpq5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/oxirescue /usr/local/bin/oxirescue

ENTRYPOINT ["oxirescue"]
