# Benchmarks

Last updated: 2026-03-12

Test machine:
- Apple M1 Pro / macOS
- `rustc 1.94.0 (4a4ef493e 2026-03-02)`

## Smoke Benchmarks

These are the benchmark artifacts that are tracked in-repo on every release-hardening pass.
They are intended to prove that benchmark code exists, compiles, and has a recorded baseline.

| Benchmark | Command | Result |
|-----------|---------|--------|
| Simple JSON response | `cargo bench -p shaperail-runtime --bench health_response` | `370.20 ns .. 383.58 ns` per request, `2.61M .. 2.70M` req/s equivalent |
| CLI release binary size | `cargo build -p shaperail-cli --release` | `3,356,272` bytes (`3.20 MiB`) |

## Release Notes

- Tagged releases must refresh this file with fresh measurements from the current commit.
- `cargo bench --workspace --no-run` is enforced in CI to keep benchmark targets compiling.
- The JSON response benchmark is an in-process handler measurement, not a network load test.
- Full PRD validation still requires dedicated DB/cache load runs on release hardware.
