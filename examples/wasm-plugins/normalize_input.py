# Example Shaperail WASM plugin — Python
#
# Normalizes input fields: trims whitespace, lowercases email.
#
# Compile to WASM using componentize-py or py2wasm:
#   pip install componentize-py
#   componentize-py -d normalize_input.py -o normalize_input.wasm
#
# Usage in resource YAML:
#   controller:
#     before: "wasm:./plugins/normalize_input.wasm"

import json

# --- Plugin interface ---
# These functions are called by the Shaperail WASM runtime.
# The actual memory management (alloc/dealloc) is handled by the
# Python-to-WASM compiler's runtime.


def before_hook(ctx_json: str) -> str:
    """
    Receives JSON context, returns JSON result.

    Context shape:
    {
        "input": {"name": "  Alice ", "email": "USER@EXAMPLE.COM"},
        "data": null,
        "user": {"id": "uuid", "role": "admin"},
        "headers": {},
        "tenant_id": null
    }

    Result shape:
    {
        "ok": true,
        "ctx": { ...modified context... }
    }
    or:
    {
        "ok": false,
        "error": "reason"
    }
    """
    ctx = json.loads(ctx_json)
    input_data = ctx.get("input", {})

    # Trim whitespace from all string fields
    for key, value in input_data.items():
        if isinstance(value, str):
            input_data[key] = value.strip()

    # Lowercase email if present
    if "email" in input_data and isinstance(input_data["email"], str):
        input_data["email"] = input_data["email"].lower()

    ctx["input"] = input_data

    return json.dumps({
        "ok": True,
        "ctx": ctx
    })
