# Stage 1: Build
#
# Compiles xzardgz in release mode against glibc with rdkafka, OpenSSL, and
# SASL support. The dependency layer is cached separately so that source
# changes do not trigger a full dependency rebuild.
FROM rust:1.87-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        clang \
        libssl-dev \
        libsasl2-dev \
        librdkafka-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache all dependency compilation behind a separate layer.
# A minimal src/main.rs satisfies cargo without requiring the real source tree.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && printf 'fn main() {}\n' > src/main.rs \
    && cargo build --release \
    && rm -f target/release/deps/xzardgz* target/release/xzardgz

# Copy the real source and build the final binary.
COPY src ./src
RUN cargo build --release

# Stage 2: Runtime
#
# Minimal Debian bookworm image containing only the runtime shared libraries
# required by xzardgz: OpenSSL (HTTPS provider calls), libsasl2 (Kafka SASL
# authentication), and ca-certificates (TLS trust roots).
FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.description="XZardgz AI-powered code review workflow harness"
LABEL org.opencontainers.image.source="https://github.com/xbcsmith/xzardgz"

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        libsasl2-2 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Run as a dedicated non-root system user.
RUN useradd --system --no-create-home --shell /usr/sbin/nologin xzardgz

COPY --from=builder /build/target/release/xzardgz /usr/local/bin/xzardgz

# /workspace is the default mount point for repository analysis.
WORKDIR /workspace

USER xzardgz

CMD ["xzardgz", "--help"]
