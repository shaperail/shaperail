/// Severity level for a diagnostic entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A static registry entry describing a single SR* diagnostic code.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct RegistryEntry {
    /// Stable error code, e.g. "SR001".
    pub code: &'static str,
    /// One-line human-readable summary of what this code means.
    pub summary: &'static str,
    /// Severity level of this diagnostic.
    pub severity: Severity,
}

/// The canonical table of all SR* diagnostic codes emitted by `shaperail-codegen`
/// and `shaperail-cli`.
///
/// SR000 (YAML parse error) is emitted by `shaperail-cli` check.rs and is listed here
/// so that `Diagnostic::error("SR000", ...)` does not trigger the debug-assert for
/// unregistered codes. SR100 (legacy INT/BIGINT drift warning) is also emitted by
/// `shaperail-cli` and is NOT listed here — it is constructed directly as a JSON value,
/// not via `Diagnostic::error`.
///
/// Keep this table sorted by code. Codes are also clustered by SR-decade
/// (SR0x = schema/field, SR2x = endpoint/inputs, SR3x = filters/cache, SR4x = controllers,
/// SR5x = upload, SR6x = relations, SR7x = indexes/auth/tenant/soft-delete/events/jobs,
/// SR8x = misc); preserve clustering when adding new codes.
///
/// When adding a new emission site in `inner.rs`, add a corresponding entry here in the
/// same commit (enforced by the `diagnostic_registry` integration test).
pub const REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        code: "SR000",
        summary: "YAML parse error",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR001",
        summary: "resource name must not be empty",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR002",
        summary: "version must be >= 1",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR003",
        summary: "schema is empty — must have at least one field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR004",
        summary: "schema has no primary key field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR005",
        summary: "schema has more than one primary key field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR010",
        summary: "field is type enum but declares no values",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR011",
        summary: "non-enum field declares values list",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR012",
        summary: "ref on non-uuid field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR013",
        summary: "ref value missing dot notation (expected resource.field)",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR014",
        summary: "array field has no items type declared",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR015",
        summary: "format attribute used on non-string field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR016",
        summary: "primary key field is neither generated nor required",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR020",
        summary: "tenant_key references a field that is not type uuid",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR021",
        summary: "tenant_key references a field not found in schema",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR030",
        summary: "controller.before has an empty hook name",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR031",
        summary: "controller.after has an empty hook name",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR032",
        summary: "events list contains an empty event name",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR033",
        summary: "jobs list contains an empty job name",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR035",
        summary: "controller hook uses 'wasm:' prefix but provides no path",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR036",
        summary: "controller hook WASM path does not end with '.wasm'",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR040",
        summary: "endpoint input/filter/search/sort references a field not in schema",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR041",
        summary: "soft_delete declared but schema has no deleted_at field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR050",
        summary: "upload declared on an endpoint whose method is not POST, PATCH, or PUT",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR051",
        summary: "upload field exists in schema but is not type file",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR052",
        summary: "upload field not found in schema",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR053",
        summary: "upload storage backend is not one of: local, s3, gcs, azure",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR054",
        summary: "upload field is not listed in the endpoint input array",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR060",
        summary: "belongs_to relation is missing required key field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR061",
        summary: "has_many or has_one relation is missing required foreign_key field",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR062",
        summary: "relation key field not found in schema",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR063",
        summary: "controller before/after list is empty",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR070",
        summary: "index definition has no fields listed",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR071",
        summary: "index references a field not found in schema",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR072",
        summary: "index order must be 'asc' or 'desc'",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR073",
        summary: "subscriber entry has an empty event pattern",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR074",
        summary: "subscriber entry has an empty handler name",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR075",
        summary: "non-convention endpoint has no handler declared",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR076",
        summary: "array field has nested array items (not supported)",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR077",
        summary: "array items type is enum but declares no values",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR078",
        summary: "array items.format is only valid when items.type is string",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR079",
        summary: "array items.ref requires items.type to be uuid",
        severity: Severity::Error,
    },
    RegistryEntry {
        code: "SR080",
        summary: "array items.ref must use 'resource.field' dot notation",
        severity: Severity::Error,
    },
];

/// Look up a registry entry by its SR code.
///
/// Returns `None` if the code is not registered.
pub fn lookup(code: &str) -> Option<&'static RegistryEntry> {
    REGISTRY.iter().find(|e| e.code == code)
}
