# ==========================================
# Stage 1: Build Environment
# ==========================================
FROM rust:slim-bookworm AS builder

WORKDIR /app

# Install build dependencies for openssl-sys (required by webauthn-rs)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests, cached sqlx queries, and source trees
COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY crates ./crates

# Tell sqlx to verify queries against .sqlx JSON metadata without hitting SQLite
ENV SQLX_OFFLINE=true

# Compile release binaries
RUN cargo build --release --workspace


# ==========================================
# Stage 2: Hardened Minimal Runtime (~45MB)
# ==========================================
FROM debian:bookworm-slim AS runtime

# Install runtime libraries for OpenSSL and CA roots
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create unprivileged kosh system group and user (UID/GID 10001)
RUN groupadd -g 10001 kosh && \
    useradd -u 10001 -g kosh -m -s /bin/sh kosh

# Setup state, configuration, and socket directories owned by kosh
RUN mkdir -p /vault /etc/kosh /tmp && \
    chown -R kosh:kosh /vault /etc/kosh

# Copy compiled binaries from builder stage
COPY --from=builder /app/target/release/server   /usr/local/bin/koshd
COPY --from=builder /app/target/release/cli      /usr/local/bin/kosh-cli

# aliases
RUN ln -s /usr/local/bin/koshd /usr/local/bin/kosh-server && \
    ln -s /usr/local/bin/koshd /usr/local/bin/server

ENV KOSH_CONFIG=/etc/kosh/config.kdl
ENV RUST_LOG=info

EXPOSE 23342/tcp
EXPOSE 5353/udp

USER kosh
WORKDIR /home/kosh

ENTRYPOINT ["/usr/local/bin/koshd"]
