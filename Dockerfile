FROM rust:1.90.0-alpine3.22

WORKDIR /app

# Install system dependencies needed for cargo-watch compilation and protobuf
RUN apk add --no-cache musl-dev gcc protobuf-dev

# Install cargo-watch for hot reloading
RUN cargo install cargo-watch

# Copy dependency files for initial caching (without full source)
# We don't copy build.rs (it moved into crates/vpr); copy workspace manifest and
# the per-crate Cargo.toml files so cargo can resolve dependencies.
COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY crates ./crates/


# Create minimal dummy source for the crates so we can cache dependencies by
# building the api package. The real source will be mounted during development.
RUN mkdir -p crates/api/src crates/vpr/src \
 && echo 'fn main(){}' > crates/api/src/main.rs \
 && echo 'pub fn dummy() {}' > crates/api/src/lib.rs \
 && echo 'fn main(){}' > crates/vpr/src/main.rs \
 && echo 'pub fn dummy() {}' > crates/vpr/src/lib.rs
# Ensure the workspace-level package (vpr-run) has a dummy binary while caching
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs

# Build only the api package to prime the dependency cache
RUN cargo build --release -p api
RUN rm -rf target/release/deps/api* target/release/api target/release/libapi*

# Don't copy real source - it will be mounted as volume

# Expose the gRPC port
EXPOSE 50051

# Use cargo watch for development
# Dependencies will be cached in the mounted target volume
CMD ["cargo", "watch", "-x", "run"]