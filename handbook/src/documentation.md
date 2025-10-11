# Documentation

postgres single entry is 22.45 ops/sec git is 8.11 ops / sec

so almost 3 times fast with postgres

test VPR server

```bash
grpcurl -plaintext \
  -import-path crates/api/proto \
  -proto crates/api/proto/vpr/v1/vpr.proto \
  -d '{}' \
  localhost:50051 vpr.v1.VPR/Health
```
