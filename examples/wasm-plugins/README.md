# Shaperail WASM Plugins

WASM plugins let you run sandboxed business logic as controller hooks.

## Plugin Interface

WASM modules must export:

| Export | Signature | Description |
|--------|-----------|-------------|
| `memory` | `(memory 2)` | Linear memory (at least 2 pages) |
| `alloc` | `(i32) -> i32` | Allocate bytes, return pointer |
| `dealloc` | `(i32, i32)` | Free memory (ptr, size) |
| `before_hook` | `(i32, i32) -> i64` | Before DB op: `(ptr, len) -> packed(ptr, len)` |
| `after_hook` | `(i32, i32) -> i64` | After DB op (optional, same signature) |

The return value is packed as `(result_ptr << 32) | result_len`.

## Context JSON (input)

```json
{
  "input": { "name": "Alice", "email": "alice@example.com" },
  "data": null,
  "user": { "id": "uuid-string", "role": "admin" },
  "headers": { "content-type": "application/json" },
  "tenant_id": null
}
```

## Result JSON (output)

Success (no changes):
```json
{"ok": true}
```

Success with modifications:
```json
{
  "ok": true,
  "ctx": {
    "input": { "name": "alice", "email": "alice@example.com" },
    "data": null,
    "user": null,
    "headers": {},
    "tenant_id": null
  }
}
```

Error:
```json
{
  "ok": false,
  "error": "validation failed: email is required"
}
```

## Usage in Resource YAML

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name]
    controller:
      before: "wasm:./plugins/validate_email.wasm"
      after: "wasm:./plugins/enrich_response.wasm"
```

## Sandboxing

Plugins run in a fully sandboxed environment:
- No filesystem access
- No network access
- No environment variables
- No system clock
- Fuel-limited execution (prevents infinite loops)
- Memory-limited (default 16MB)

## Compiling Plugins

### From TypeScript (AssemblyScript)
```bash
npm install -g assemblyscript
asc validate_email.ts --outFile validate_email.wasm --exportRuntime
```

### From Python (componentize-py)
```bash
pip install componentize-py
componentize-py -d normalize_input.py -o normalize_input.wasm
```

### From Rust
```bash
cargo build --target wasm32-unknown-unknown --release
```

### From WAT (WebAssembly Text)
```bash
wat2wasm plugin.wat -o plugin.wasm
```
