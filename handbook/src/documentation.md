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

```bash
grpcurl -plaintext \
  -import-path crates/api/proto \
  -proto crates/api/proto/vpr/v1/vpr.proto \
  -d '{}' \
  localhost:50051 vpr.v1.VPR/Health
```
