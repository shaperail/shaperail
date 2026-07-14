Run the full Shaperail quality gate and fix failures before stopping.

1. Check disk space with `df -h . | tail -1`; do not begin a long
   build with less than about 10 GB available.
2. Run `cargo fmt --check`.
3. Run `cargo build --workspace`.
4. Run `cargo clippy --workspace --all-targets -- -D warnings`.
5. Run `cargo test --workspace` with the repository's PostgreSQL and Redis
   environment available.
6. Scan non-test production Rust for `.unwrap()` and `.expect(` and review every
   result. Do not flag safe combinators such as `unwrap_or`.
7. Confirm generated SQL still uses compile-time checked macros; dynamic sqlx
   calls are allowed only in generic runtime paths where schemas are not known
   at compile time.
8. Run `docker compose config --quiet`.

Do not delete a failing test or suppress a warning to make the gate green.
