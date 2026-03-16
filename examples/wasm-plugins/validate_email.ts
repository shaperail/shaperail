// Example Shaperail WASM plugin — TypeScript
//
// Validates that the "email" field contains "@" before the record is saved.
//
// Compile to WASM using AssemblyScript:
//   npm install -g assemblyscript
//   asc validate_email.ts --outFile validate_email.wasm --exportRuntime
//
// Usage in resource YAML:
//   controller:
//     before: "wasm:./plugins/validate_email.wasm"

// --- Plugin interface (must be exported) ---

// Allocate memory for the host to write into.
// @ts-ignore: decorator
@external("env", "memory")
declare const memory: WebAssembly.Memory;

let bumpPtr: usize = 4096;

export function alloc(size: i32): i32 {
  const ptr = bumpPtr;
  bumpPtr += size;
  return ptr as i32;
}

export function dealloc(_ptr: i32, _size: i32): void {
  // no-op for bump allocator
}

// before_hook receives JSON context, returns packed (ptr << 32) | len
export function before_hook(ptr: i32, len: i32): i64 {
  // Read input JSON from memory
  const inputBytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) {
    inputBytes[i] = load<u8>(ptr + i);
  }

  const input = String.UTF8.decode(inputBytes.buffer);
  const ctx = JSON.parse(input);

  // Validate email field
  const email = ctx.input?.email;
  if (email && !email.includes("@")) {
    const error = JSON.stringify({
      ok: false,
      error: "Invalid email: must contain '@'"
    });
    return writeResult(error);
  }

  // Pass through — no modifications
  const ok = JSON.stringify({ ok: true });
  return writeResult(ok);
}

function writeResult(json: string): i64 {
  const bytes = String.UTF8.encode(json);
  const outPtr = 0;
  const view = new Uint8Array(bytes);
  for (let i = 0; i < view.length; i++) {
    store<u8>(outPtr + i, view[i]);
  }
  // Pack: (ptr << 32) | len
  return (i64(outPtr) << 32) | i64(view.length);
}
