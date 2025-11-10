# VPR - Versioned Patient Repository

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![gRPC](https://img.shields.io/badge/gRPC-4285F4?style=for-the-badge&logo=google&logoColor=white)

Install pre-commit hooks

```bash
pre-commit install
```

install rust locally if you want to test on local machine

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

start a new terminal to be able to use rust

Install protobuf compiler

```bash
brew install protobuf
```

Build

```bash
cargo build
```

## Nuke Docs

As the docs run on a cache, you will likely need to nuke the docs if you remove files. Just manually run `nuke docs cache (manual)` from GitHub Actions.

# Documentation

postgres single entry is 22.45 ops/sec git is 8.11 ops / sec

so almost 3 times fast with postgres

## Time trial

```bash
brew install hyperfine
brew install postgresql@16
brew services start postgresql@16
PGURL="postgres://user:pass@localhost:5432/postgres" N=10000 ./file_db_time_trial.sh
createuser -s postgres || true
psql -U postgres -c "ALTER USER postgres WITH PASSWORD 'postgres';" || true
```

## Test VPR server

With server reflection enabled (set `VPR_ENABLE_REFLECTION=true`), you can use:

```bash
grpcurl -plaintext -d '{}' localhost:50051 vpr.v1.VPR/Health
```

To get a reflection of the service:

```bash
grpcurl -plaintext localhost:50051 describe vpr.v1.VPR
```

You can check out endpoints specifics like this:

```bash
grpcurl -plaintext localhost:50051 describe .vpr.v1.CreatePatientReq
```

Or with the proto file (without reflection):

```bash
grpcurl -plaintext \
  -import-path crates/api/proto \
  -proto crates/api/proto/vpr/v1/vpr.proto \
  -d '{}' \
  localhost:50051 vpr.v1.VPR/Health
```

Note: Server reflection is disabled by default for security in production. Set `VPR_ENABLE_REFLECTION=true` to enable it.
