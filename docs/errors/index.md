---
title: Error Codes
nav_order: 90
has_children: true
---

# Error Codes

Every diagnostic emitted by `shaperail check` has a stable code (`SR001`,
`SR042`, ...) and a permanent reference page on this site.

When `shaperail check --json` reports a diagnostic, the `doc_url` field
points at the corresponding page below.

| Code  | Severity | Summary |
|-------|----------|---------|
| [SR000](SR000.md) | error | YAML parse error |
| [SR001](SR001.md) | error | resource name must not be empty |
| [SR002](SR002.md) | error | version must be >= 1 |
| [SR003](SR003.md) | error | schema is empty — must have at least one field |
| [SR004](SR004.md) | error | schema has no primary key field |
| [SR005](SR005.md) | error | schema has more than one primary key field |
| [SR010](SR010.md) | error | field is type enum but declares no values |
| [SR011](SR011.md) | error | non-enum field declares values list |
| [SR012](SR012.md) | error | ref on non-uuid field |
| [SR013](SR013.md) | error | ref value missing dot notation (expected resource.field) |
| [SR014](SR014.md) | error | array field has no items type declared |
| [SR015](SR015.md) | error | format attribute used on non-string field |
| [SR016](SR016.md) | error | primary key field is neither generated nor required |
| [SR020](SR020.md) | error | tenant_key references a field that is not type uuid |
| [SR021](SR021.md) | error | tenant_key references a field not found in schema |
| [SR030](SR030.md) | error | controller.before has an empty hook name |
| [SR031](SR031.md) | error | controller.after has an empty hook name |
| [SR032](SR032.md) | error | events list contains an empty event name |
| [SR033](SR033.md) | error | jobs list contains an empty job name |
| [SR035](SR035.md) | error | controller hook uses 'wasm:' prefix but provides no path |
| [SR036](SR036.md) | error | controller hook WASM path does not end with '.wasm' |
| [SR040](SR040.md) | error | endpoint input/filter/search/sort references a field not in schema |
| [SR041](SR041.md) | error | soft_delete declared but schema has no deleted_at field |
| [SR050](SR050.md) | error | upload declared on an endpoint whose method is not POST, PATCH, or PUT |
| [SR051](SR051.md) | error | upload field exists in schema but is not type file |
| [SR052](SR052.md) | error | upload field not found in schema |
| [SR053](SR053.md) | error | upload storage backend is not one of: local, s3, gcs, azure |
| [SR054](SR054.md) | error | upload field is not listed in the endpoint input array |
| [SR060](SR060.md) | error | belongs_to relation is missing required key field |
| [SR061](SR061.md) | error | has_many or has_one relation is missing required foreign_key field |
| [SR062](SR062.md) | error | relation key field not found in schema |
| [SR063](SR063.md) | error | controller before/after list is empty |
| [SR070](SR070.md) | error | index definition has no fields listed |
| [SR071](SR071.md) | error | index references a field not found in schema |
| [SR072](SR072.md) | error | index order must be 'asc' or 'desc' |
| [SR073](SR073.md) | error | subscriber entry has an empty event pattern |
| [SR074](SR074.md) | error | subscriber entry has an empty handler name |
| [SR075](SR075.md) | error | non-convention endpoint has no handler declared |
| [SR076](SR076.md) | error | array field has nested array items (not supported) |
| [SR077](SR077.md) | error | array items type is enum but declares no values |
| [SR078](SR078.md) | error | array items.format is only valid when items.type is string |
| [SR079](SR079.md) | error | array items.ref requires items.type to be uuid |
| [SR080](SR080.md) | error | array items.ref must use 'resource.field' dot notation |
