Add a Shaperail resource named `$ARGUMENTS`.

1. Read `agent_docs/resource-format.md`.
2. From a Shaperail project root, run
   `shaperail resource create <name> --archetype basic`.
3. Edit `resources/<name>.yaml`; keep standard CRUD method/path defaults and
   declare every client-writable field in each write endpoint's `input`.
4. Put custom request-time logic in
   `resources/<name>.controller.rs` using `controller.before` or
   `controller.after`. Never edit `generated/`.
5. Run `shaperail check resources/<name>.yaml --json` and fix every diagnostic.
6. Run `shaperail generate`, then `shaperail test`.

Use a different supported archetype only when `$ARGUMENTS` explicitly requests
one: `user`, `content`, `tenant`, or `lookup`.
