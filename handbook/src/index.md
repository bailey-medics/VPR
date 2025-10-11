# VPR - Versioned Patient Repository

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![gRPC](https://img.shields.io/badge/gRPC-4285F4?style=for-the-badge&logo=google&logoColor=white)

A play with VPR openEHR in server form - using rust only

---- API 22 ----

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

## Time trial

```bash
brew install hyperfine
brew install postgresql@16
brew services start postgresql@16
PGURL="postgres://user:pass@localhost:5432/postgres" N=10000 ./file_db_time_trial.sh
createuser -s postgres || true
psql -U postgres -c "ALTER USER postgres WITH PASSWORD 'postgres';" || true
```
