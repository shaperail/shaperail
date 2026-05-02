---
title: Reject undeclared bracket-notation filter params
nav_exclude: true
---

# Reject undeclared bracket-notation filter params (Issue H)

**Status:** approved 2026-05-03
**Target release:** 0.12.0 (pre-1.0 minor bump because the fix promotes silent ignore to a 422 — breaking)
**Closes:** Issue H

## Problem

`shaperail-runtime` 0.11.3 added `validate_filter_param_form`, which catches bare `?field=value` query params on list endpoints when `field` is in the endpoint's declared `filters:` list and returns 422 `INVALID_FILTER_FORM` with a "did you mean `?filter[field]=...`?" hint. Closed Issue G.

The complementary footgun is still open. When a caller uses the **correct** bracket notation `?filter[field]=value` but `field` is **not** in the declared `filters:` list, the runtime accepts the URL, returns 200, and silently drops the predicate. The response is paginated, structurally correct, and unfiltered. The caller cannot tell their predicate was ignored without inspecting row contents and noticing they don't match the filter.

This is the same anti-pattern Issue G fixed, one indirection deeper. The caller did everything right syntactically; they just didn't know each filterable field has to be opted in via YAML.

## Reproduction

```yaml
resource: journal_entries
endpoints:
  list:
    method: GET
    path: /journal_entries
    filters: [posting_date, source, source_id]   # `org_id` deliberately omitted
```

```
curl "http://localhost:3000/v1/journal_entries?filter[org_id]=$KNOWN_UUID"
→ 200 OK with the first page of journal_entries across ALL orgs
```

The `filter[org_id]` predicate is silently dropped.

## Goal

Promote the silent ignore to a loud 422, mirroring the 0.11.3 fix. Caller learns immediately that the predicate didn't apply, with a message naming the available filters so they can fix the URL or the resource YAML.

## Design

### Code change

Single function: `shaperail-runtime/src/handlers/params.rs::validate_filter_param_form`.

1. Drop the early-return `if allowed.is_empty() { return Ok(()) }` so the function still inspects bracket-notation params on endpoints with no declared filters.
2. After the existing bare-field check (which already 422s when a bare key matches a declared filter), add a second check: if the key has the shape `filter[<field>]` and `<field>` is not in `allowed`, push a `FieldError` with code `UNDECLARED_FILTER`.
3. Both error families accumulate into the same `Vec<FieldError>` so a request with multiple problems gets one 422 listing all of them.

### Error contract

| Field | Value |
|---|---|
| HTTP status | 422 (existing `ShaperailError::Validation` mapping) |
| Top-level code | `VALIDATION_ERROR` (existing) |
| Per-field `code` | `UNDECLARED_FILTER` |
| Per-field `field` | `filter[<field>]` — preserved as the caller wrote it |
| Per-field `message` (filters declared) | `'<field>' is not a declared filter on this endpoint; available filters: <comma-separated list>` |
| Per-field `message` (no filters declared) | `this endpoint declares no filters; '<field>' is not accepted` |

The pre-existing `INVALID_FILTER_FORM` error contract for bare-field params is unchanged.

### Test plan

Five unit tests in `shaperail-runtime/src/handlers/params.rs::tests`:

1. `validate_filter_param_form_rejects_bracket_notation_on_undeclared_field` — `?filter[org_id]=...` with `filters: [posting_date]` → 422 with `UNDECLARED_FILTER` and message naming `posting_date`.
2. `validate_filter_param_form_rejects_bracket_notation_when_no_filters_declared` — `?filter[anything]=...` with no `filters:` → 422 with the "no filters declared" message variant.
3. `validate_filter_param_form_accumulates_multiple_errors` — request with one bare-field-match and one undeclared-bracket → single 422 with two `FieldError` entries (one `INVALID_FILTER_FORM`, one `UNDECLARED_FILTER`).
4. Existing `validate_filter_param_form_accepts_bracket_notation` (declared filter, bracket form) stays green.
5. Existing `validate_filter_param_form_ignores_unrelated_bare_params` and `validate_filter_param_form_is_noop_when_no_filters_declared` are reviewed — the latter is **renamed/repurposed** since the no-filters path now flags bracket-notation params (its current assertion that the function returns `Ok` for `?role=admin` with no filters declared is still valid because `role` is bare, not bracket).

### Docs

- `docs/api-responses.md` — extend the existing "Bare-field params are rejected" callout with a sibling paragraph describing `UNDECLARED_FILTER` and an example.
- `agent_docs/resource-format.md` — extend the `filters` row reference to cite both error codes (`INVALID_FILTER_FORM`, `UNDECLARED_FILTER`).
- `CHANGELOG.md` `[Unreleased]` — Breaking entry "Closes Issue H".

### Conventional commit

`feat!: reject bracket-notation filter params for undeclared fields`

Calls that previously returned 200 unfiltered now return 422. Pre-1.0 semver convention → minor bump (`0.11.3` → `0.12.0`).

## Non-goals

- **`meta.applied_filters` in responses.** The bug report mentions this as an alternative. Out of scope here — it's a response-shape change that affects every list endpoint and warrants its own design. The 422 alone closes the footgun.
- **Differentiating "field exists in schema but not filters" from "field unknown".** The user's report explicitly says "same message". Same code, same message — simpler implementation, simpler caller logic. The caller's fix is the same in both cases (add the field to `filters:` if it's a real schema field, or fix the typo if it isn't).
- **Validating that `<field>` is a real schema field.** The runtime doesn't have a cheap way to do that here without plumbing the resource definition deeper into `validate_filter_param_form`. Listing available filters in the message gives the caller enough information to self-diagnose.
- **Strengthening codegen-time validation that `filters:` entries are real schema fields.** Out of scope; that's a separate codegen check.

## Risks and mitigations

- **Existing clients that rely on the silent-ignore behavior break.** This is the breaking change semantic. Mitigated by the explicit error message, the `[Unreleased]` Breaking changelog entry, and the conventional `feat!:` prefix.
- **Custom endpoints (non-list) inadvertently subject to this check.** `validate_filter_param_form` is only called from `crud.rs::execute_list`. Custom endpoints reach `dispatch_custom_handler` instead and never hit this function. Confirmed in code.

## Out-of-scope follow-ups (don't bundle)

- A new `meta.applied_filters` response field for client-side verification of which predicates ran.
- A codegen validator that rejects `filters: [<field>]` entries that aren't real schema fields.
