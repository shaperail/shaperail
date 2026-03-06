Run the full SteelAPI quality gate. Do not stop until everything passes.

Execute in this exact order:

1. `cargo fmt --check`
   - If fails: run `cargo fmt` then re-check

2. `cargo build --workspace`
   - If fails: fix ALL errors before continuing — read every error line

3. `cargo clippy --workspace -- -D warnings`
   - If fails: fix every warning. No `#[allow(...)]` without a comment explaining why
   - Exception allowed: `clippy::too_many_arguments` on Actix handler functions

4. `cargo test --workspace`
   - If fails: fix the code or test — never delete a failing test
   - Run `cargo test <test_name> -- --nocapture` for full output

5. Check the Five Design Rules — grep for violations:
   - `grep -r "\.unwrap()\|\.expect(" --include="*.rs" $(find . -name "*.rs" ! -path "*/tests/*" ! -path "*_test*")`
     → Any result outside test files = VIOLATION of Rule 5. Fix before continuing.
   - `grep -r "query(" --include="*.rs" steel-runtime/src/`
     → Raw query() calls = VIOLATION of Rule 4. Must use sqlx::query_as! macro.

6. Verify Docker dev environment works:
   - `docker compose config` — validates docker-compose.yml syntax

Report: what passed, what failed, what was fixed.
If all pass, print: "✅ All checks passed — safe to commit"
