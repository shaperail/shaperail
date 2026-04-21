# LLM Context Files in Scaffold — Design Spec
**Date:** 2026-04-21
**Status:** Approved
**Scope:** Add LLM context files to `shaperail init` scaffold so every coding agent has Shaperail syntax knowledge from day one.

---

## Problem

`shaperail init my-app` produces a working project but no AI context files. A developer who opens the scaffolded project in Cursor, Claude Code, GitHub Copilot, Windsurf, or any other coding agent gets zero Shaperail framework knowledge. The AI will hallucinate resource syntax, use wrong key names, and generate invalid YAML on the first request.

The framework already has a complete, accurate `docs/llm-guide.md` (366 lines) and `docs/llm-reference.md` (125 lines) but they only live on the public website — not in user projects.

---

## Out of Scope

- Changing `shaperail context` command behavior
- Downloading/updating context files after scaffold (no network dependency)
- Adding a `shaperail context --full` combined output command
- Per-resource context files

---

## Design

### Files Added to Every Scaffolded Project

```
my-app/
  llm-context.md                       ← canonical full syntax reference
  CLAUDE.md                            ← Claude Code
  AGENTS.md                            ← Codex / OpenAI agents
  GEMINI.md                            ← Gemini CLI
  .cursor/rules/shaperail.md           ← Cursor
  .github/copilot-instructions.md      ← GitHub Copilot
  .windsurfrules                       ← Windsurf
```

### `llm-context.md` — Canonical Content

Full `docs/llm-guide.md` + `docs/llm-reference.md` content, stripped of Jekyll frontmatter (`---` blocks), with this header prepended:

```markdown
# Shaperail LLM Context

This project uses the Shaperail framework (deterministic Rust backend from YAML resources).

**Live project state:** Run `shaperail context` to see the current resources, schema, and endpoints for this specific project.

**IDE validation:** Add `# yaml-language-server: $schema=./resources/.schema.json` as the first line of any resource YAML file for inline validation.

---
```

Content is a Rust `const &str` embedded in the CLI binary — zero network dependency, version-locked to the framework release, works offline.

### Agent Adapter Files — Identical Content

All six agent files (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `.cursor/rules/shaperail.md`, `.github/copilot-instructions.md`, `.windsurfrules`) share the same template:

```markdown
This is a Shaperail project — a deterministic Rust backend framework driven by YAML resource files.

**Full syntax reference:** See `./llm-context.md`
**Live project state:** Run `shaperail context` to see current resources, schema, and endpoints.

## Key Rules

- One canonical syntax per concept — no aliases, no alternative forms
- `resource:` is the top-level key (not `name:`)
- Resource YAML is the source of truth; never reverse-generate it from code
- Unknown fields in resource YAML cause a loud compile error
- `shaperail check --json` gives structured diagnostics with fix suggestions
```

Six files, same content, each in the path their respective tool expects.

### Implementation in `init.rs`

**New constant:**
```rust
const LLM_CONTEXT_MD: &str = "..."; // merged llm-guide + llm-reference, frontmatter stripped
```

**New adapter constant (shared across all agent files):**
```rust
const AGENT_ADAPTER_MD: &str = "..."; // the ~15-line template above
```

**New `write_file` calls at end of `scaffold()`:**
```rust
// LLM context files for coding agents
write_file(&root.join("llm-context.md"), LLM_CONTEXT_MD)?;
write_file(&root.join("CLAUDE.md"), AGENT_ADAPTER_MD)?;
write_file(&root.join("AGENTS.md"), AGENT_ADAPTER_MD)?;
write_file(&root.join("GEMINI.md"), AGENT_ADAPTER_MD)?;
fs::create_dir_all(root.join(".cursor/rules"))
    .map_err(|e| format!("Failed to create .cursor/rules: {e}"))?;
write_file(&root.join(".cursor/rules/shaperail.md"), AGENT_ADAPTER_MD)?;
fs::create_dir_all(root.join(".github"))
    .map_err(|e| format!("Failed to create .github: {e}"))?;
write_file(&root.join(".github/copilot-instructions.md"), AGENT_ADAPTER_MD)?;
write_file(&root.join(".windsurfrules"), AGENT_ADAPTER_MD)?;
```

`fs::create_dir_all` is already imported in `init.rs` via `use std::fs`.

### Tests

One new test in `shaperail-cli/tests/cli_tests.rs`:

```rust
#[test]
fn scaffold_writes_llm_context_files() {
    let dir = tempfile::tempdir().unwrap();
    // call scaffold() directly or via the test helper that already exists
    scaffold("test-app", dir.path()).unwrap();
    let root = dir.path();
    assert!(root.join("llm-context.md").exists(), "llm-context.md missing");
    assert!(root.join("CLAUDE.md").exists(), "CLAUDE.md missing");
    assert!(root.join("AGENTS.md").exists(), "AGENTS.md missing");
    assert!(root.join("GEMINI.md").exists(), "GEMINI.md missing");
    assert!(root.join(".cursor/rules/shaperail.md").exists(), ".cursor/rules/shaperail.md missing");
    assert!(root.join(".github/copilot-instructions.md").exists(), ".github/copilot-instructions.md missing");
    assert!(root.join(".windsurfrules").exists(), ".windsurfrules missing");

    // spot-check content
    let claude = fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    assert!(claude.contains("llm-context.md"), "CLAUDE.md should reference llm-context.md");
    let ctx = fs::read_to_string(root.join("llm-context.md")).unwrap();
    assert!(ctx.contains("shaperail context"), "llm-context.md should mention shaperail context command");
}
```

---

## Files Changed

| File | Change |
|------|--------|
| `shaperail-cli/src/commands/init.rs` | Add `LLM_CONTEXT_MD` and `AGENT_ADAPTER_MD` constants; add 9 new `write_file`/`create_dir_all` calls in `scaffold()` |
| `shaperail-cli/tests/cli_tests.rs` | Add `scaffold_writes_llm_context_files` test |

No other files changed. No new crates. No new dependencies.
