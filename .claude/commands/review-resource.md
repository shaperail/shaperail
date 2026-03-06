Review the resource file: $ARGUMENTS

Read agent_docs/resource-format.md first.

Check `resources/$ARGUMENTS` against these rules from the PRD:

**Format Rules (Rule 1 — One Way)**
- [ ] Top-level key is `resource:` not `name:` or `entity:`
- [ ] `version:` field present
- [ ] Fields use inline format: `field: { type: x, constraint: y }`
- [ ] Endpoint keys include `method:` and `path:`
- [ ] `auth:` is an array of role strings, or `public`

**Schema Rules (Rule 4 — Schema is Source of Truth)**
- [ ] Every resource has `id: { type: uuid, primary: true, generated: true }`
- [ ] Every resource has `created_at` and `updated_at` with `generated: true`
- [ ] Enum fields have `values: [...]`
- [ ] Foreign keys use `ref: resource.id` format
- [ ] `sensitive: true` on email, phone, password_hash, SSN fields

**Endpoint Rules (Rule 2 — Explicit over Implicit)**
- [ ] Every endpoint declares `method:` and `path:`
- [ ] Write endpoints declare `input: [...]` explicitly
- [ ] Hooks referenced by name match functions in hooks/<resource>.hooks.rs
- [ ] Events follow `resource.verb` naming (e.g. `user.created`)

**PRD Success Metric Check**
- [ ] Would the codegen for this resource produce a compilable Rust file?
- [ ] Is the auth correct for the data sensitivity level?
- [ ] Are indexes declared for all foreign keys and frequently-filtered fields?

Report findings as: ✅ correct / ⚠️ should fix / ❌ must fix
