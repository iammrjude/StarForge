FROM rust:1-bookworm

WORKDIR /workspace

# Install system deps commonly needed for Rust + TLS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    pkg-config \
    libssl-dev \
    curl \
 && rm -rf /var/lib/apt/lists/*

# Optional: install Stellar CLI (used by `starforge shell` sandbox execution).
# If the upstream install script changes, users can still use the container without it.
RUN (curl -fsSL https://stellar.org/install.sh | bash) || true

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY . .

CMD ["bash"]

