//! Proto file generation from resource definitions (M16).
//!
//! Generates `.proto` files from `ResourceDefinition`s. Each resource produces
//! a service with CRUD RPCs including server-streaming for list endpoints.

use shaperail_core::{FieldType, ResourceDefinition};

/// Maps a Shaperail `FieldType` to a protobuf type string.
fn field_type_to_proto(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Uuid => "string",
        FieldType::String => "string",
        FieldType::Integer => "int32",
        FieldType::Bigint => "int64",
        FieldType::Number => "double",
        FieldType::Boolean => "bool",
        FieldType::Timestamp => "google.protobuf.Timestamp",
        FieldType::Date => "string",
        FieldType::Enum => "string",
        FieldType::Json => "google.protobuf.Struct",
        FieldType::Array => "google.protobuf.ListValue",
        FieldType::File => "string",
    }
}

/// Returns true if the field type requires a well-known-types import.
pub fn needs_wkt_import(ft: &FieldType) -> bool {
    matches!(
        ft,
        FieldType::Timestamp | FieldType::Json | FieldType::Array
    )
}

/// Converts a snake_case resource name to PascalCase for protobuf message names.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Converts a plural resource name to a singular form for message naming.
/// Simple heuristic: strip trailing 's' if present.
fn to_singular(s: &str) -> String {
    // Words that end in 's' but are not plural
    const EXCEPTIONS: &[&str] = &["status", "bus", "alias", "canvas"];
    if EXCEPTIONS.iter().any(|e| s.ends_with(e)) {
        return s.to_string();
    }

    if let Some(stripped) = s.strip_suffix("ies") {
        format!("{stripped}y")
    } else if s.ends_with("ses") || s.ends_with("xes") || s.ends_with("zes") {
        // Strip "es" from "addresses", "boxes", "buzzes" etc.
        s[..s.len() - 2].to_string()
    } else if let Some(stripped) = s.strip_suffix('s') {
        if stripped.ends_with('s') {
            s.to_string()
        } else {
            stripped.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Generates a `.proto` file content from a single `ResourceDefinition`.
///
/// The generated proto includes:
/// - A message type for the resource with all schema fields
/// - Create/Update input messages based on endpoint `input` fields
/// - Request/Response wrappers for each endpoint
/// - A gRPC service with RPCs for each declared endpoint
/// - Server-streaming RPC for list endpoints
pub fn generate_proto(resource: &ResourceDefinition) -> String {
    let resource_name = &resource.resource;
    let singular = to_singular(resource_name);
    let pascal = to_pascal_case(&singular);
    let pascal_plural = to_pascal_case(resource_name);
    let version = resource.version;

    let mut needs_timestamp = false;
    let mut needs_struct = false;

    // Check if we need WKT imports
    for field in resource.schema.values() {
        if matches!(field.field_type, FieldType::Timestamp) {
            needs_timestamp = true;
        }
        if matches!(field.field_type, FieldType::Json | FieldType::Array) {
            needs_struct = true;
        }
    }

    let mut proto = String::new();
    proto.push_str("syntax = \"proto3\";\n\n");
    proto.push_str(&format!(
        "package shaperail.v{version}.{resource_name};\n\n"
    ));

    if needs_timestamp {
        proto.push_str("import \"google/protobuf/timestamp.proto\";\n");
    }
    if needs_struct {
        proto.push_str("import \"google/protobuf/struct.proto\";\n");
    }
    if needs_timestamp || needs_struct {
        proto.push('\n');
    }

    // Resource message
    proto.push_str(&format!("// {pascal} resource message.\n"));
    proto.push_str(&format!("message {pascal} {{\n"));
    for (i, (field_name, field_schema)) in resource.schema.iter().enumerate() {
        let proto_type = field_type_to_proto(&field_schema.field_type);
        let field_num = i + 1;
        if field_schema.field_type == FieldType::Enum {
            if let Some(ref values) = field_schema.values {
                proto.push_str(&format!("  // Allowed values: {}\n", values.join(", ")));
            }
        }
        proto.push_str(&format!("  {proto_type} {field_name} = {field_num};\n"));
    }
    proto.push_str("}\n\n");

    // Determine endpoints
    let endpoints = resource.endpoints.as_ref();
    let has_list = endpoints.and_then(|e| e.get("list")).is_some();
    let has_get = endpoints.and_then(|e| e.get("get")).is_some();
    let has_create = endpoints.and_then(|e| e.get("create")).is_some();
    let has_update = endpoints.and_then(|e| e.get("update")).is_some();
    let has_delete = endpoints.and_then(|e| e.get("delete")).is_some();

    // List request/response
    if has_list {
        proto.push_str(&format!("message List{pascal_plural}Request {{\n"));
        // Add filter fields from the list endpoint
        if let Some(ep) = endpoints.and_then(|e| e.get("list")) {
            let mut field_num = 1;
            if let Some(ref filters) = ep.filters {
                for f in filters {
                    proto.push_str(&format!("  string {f} = {field_num};\n"));
                    field_num += 1;
                }
            }
            if ep.search.is_some() {
                proto.push_str(&format!("  string search = {field_num};\n"));
                field_num += 1;
            }
            proto.push_str(&format!("  string cursor = {field_num};\n"));
            field_num += 1;
            proto.push_str(&format!("  int32 page_size = {field_num};\n"));
            field_num += 1;
            proto.push_str(&format!("  string sort = {field_num};\n"));
        }
        proto.push_str("}\n\n");

        proto.push_str(&format!("message List{pascal_plural}Response {{\n"));
        proto.push_str(&format!("  repeated {pascal} items = 1;\n"));
        proto.push_str("  string next_cursor = 2;\n");
        proto.push_str("  bool has_more = 3;\n");
        proto.push_str("  int64 total = 4;\n");
        proto.push_str("}\n\n");
    }

    // Get request/response
    if has_get {
        proto.push_str(&format!("message Get{pascal}Request {{\n"));
        proto.push_str("  string id = 1;\n");
        proto.push_str("}\n\n");

        proto.push_str(&format!("message Get{pascal}Response {{\n"));
        proto.push_str(&format!("  {pascal} data = 1;\n"));
        proto.push_str("}\n\n");
    }

    // Create request/response
    if has_create {
        proto.push_str(&format!("message Create{pascal}Request {{\n"));
        if let Some(ep) = endpoints.and_then(|e| e.get("create")) {
            if let Some(ref input) = ep.input {
                for (i, field_name) in input.iter().enumerate() {
                    let proto_type = resource
                        .schema
                        .get(field_name.as_str())
                        .map(|f| field_type_to_proto(&f.field_type))
                        .unwrap_or("string");
                    proto.push_str(&format!("  {proto_type} {field_name} = {};\n", i + 1));
                }
            }
        }
        proto.push_str("}\n\n");

        proto.push_str(&format!("message Create{pascal}Response {{\n"));
        proto.push_str(&format!("  {pascal} data = 1;\n"));
        proto.push_str("}\n\n");
    }

    // Update request/response
    if has_update {
        proto.push_str(&format!("message Update{pascal}Request {{\n"));
        proto.push_str("  string id = 1;\n");
        if let Some(ep) = endpoints.and_then(|e| e.get("update")) {
            if let Some(ref input) = ep.input {
                for (i, field_name) in input.iter().enumerate() {
                    let proto_type = resource
                        .schema
                        .get(field_name.as_str())
                        .map(|f| field_type_to_proto(&f.field_type))
                        .unwrap_or("string");
                    proto.push_str(&format!("  {proto_type} {field_name} = {};\n", i + 2));
                }
            }
        }
        proto.push_str("}\n\n");

        proto.push_str(&format!("message Update{pascal}Response {{\n"));
        proto.push_str(&format!("  {pascal} data = 1;\n"));
        proto.push_str("}\n\n");
    }

    // Delete request/response
    if has_delete {
        proto.push_str(&format!("message Delete{pascal}Request {{\n"));
        proto.push_str("  string id = 1;\n");
        proto.push_str("}\n\n");

        proto.push_str(&format!("message Delete{pascal}Response {{\n"));
        proto.push_str("  bool success = 1;\n");
        proto.push_str("}\n\n");
    }

    // Service definition
    proto.push_str(&format!(
        "// gRPC service for {resource_name} (v{version}).\n"
    ));
    proto.push_str(&format!("service {pascal}Service {{\n"));

    if has_list {
        proto.push_str(&format!(
            "  // Lists {resource_name} with filters, pagination, and sorting.\n"
        ));
        proto.push_str(&format!(
            "  rpc List{pascal_plural}(List{pascal_plural}Request) returns (List{pascal_plural}Response);\n\n"
        ));
        proto.push_str(&format!(
            "  // Streams {resource_name} matching the request filters.\n"
        ));
        proto.push_str(&format!(
            "  rpc Stream{pascal_plural}(List{pascal_plural}Request) returns (stream {pascal});\n\n"
        ));
    }

    if has_get {
        proto.push_str(&format!("  // Gets a single {singular} by ID.\n"));
        proto.push_str(&format!(
            "  rpc Get{pascal}(Get{pascal}Request) returns (Get{pascal}Response);\n\n"
        ));
    }

    if has_create {
        proto.push_str(&format!("  // Creates a new {singular}.\n"));
        proto.push_str(&format!(
            "  rpc Create{pascal}(Create{pascal}Request) returns (Create{pascal}Response);\n\n"
        ));
    }

    if has_update {
        proto.push_str(&format!("  // Updates an existing {singular}.\n"));
        proto.push_str(&format!(
            "  rpc Update{pascal}(Update{pascal}Request) returns (Update{pascal}Response);\n\n"
        ));
    }

    if has_delete {
        proto.push_str(&format!("  // Deletes a {singular} by ID.\n"));
        proto.push_str(&format!(
            "  rpc Delete{pascal}(Delete{pascal}Request) returns (Delete{pascal}Response);\n"
        ));
    }

    proto.push_str("}\n");

    proto
}

/// Generates proto files for all resources, returning `(filename, content)` pairs.
pub fn generate_all_protos(resources: &[ResourceDefinition]) -> Vec<(String, String)> {
    resources
        .iter()
        .map(|r| {
            let filename = format!("{}.proto", r.resource);
            let content = generate_proto(r);
            (filename, content)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_pascal_case_simple() {
        assert_eq!(to_pascal_case("users"), "Users");
        assert_eq!(to_pascal_case("blog_posts"), "BlogPosts");
        assert_eq!(to_pascal_case("api_keys"), "ApiKeys");
    }

    #[test]
    fn to_singular_simple() {
        assert_eq!(to_singular("users"), "user");
        assert_eq!(to_singular("blog_posts"), "blog_post");
        assert_eq!(to_singular("categories"), "category");
        assert_eq!(to_singular("addresses"), "address");
        assert_eq!(to_singular("status"), "status");
    }

    #[test]
    fn field_type_mapping() {
        assert_eq!(field_type_to_proto(&FieldType::Uuid), "string");
        assert_eq!(field_type_to_proto(&FieldType::String), "string");
        assert_eq!(field_type_to_proto(&FieldType::Integer), "int32");
        assert_eq!(field_type_to_proto(&FieldType::Bigint), "int64");
        assert_eq!(field_type_to_proto(&FieldType::Number), "double");
        assert_eq!(field_type_to_proto(&FieldType::Boolean), "bool");
        assert_eq!(
            field_type_to_proto(&FieldType::Timestamp),
            "google.protobuf.Timestamp"
        );
        assert_eq!(field_type_to_proto(&FieldType::Date), "string");
        assert_eq!(field_type_to_proto(&FieldType::Enum), "string");
        assert_eq!(
            field_type_to_proto(&FieldType::Json),
            "google.protobuf.Struct"
        );
    }

    #[test]
    fn wkt_import_detection() {
        assert!(needs_wkt_import(&FieldType::Timestamp));
        assert!(needs_wkt_import(&FieldType::Json));
        assert!(needs_wkt_import(&FieldType::Array));
        assert!(!needs_wkt_import(&FieldType::String));
        assert!(!needs_wkt_import(&FieldType::Uuid));
    }

    use indexmap::IndexMap;
    use shaperail_core::{EndpointSpec, FieldSchema, HttpMethod};

    fn field(ft: FieldType) -> FieldSchema {
        FieldSchema {
            field_type: ft,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
            transient: false,
        }
    }

    fn endpoint(method: HttpMethod, path: &str) -> EndpointSpec {
        EndpointSpec {
            method: Some(method),
            path: Some(path.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn generate_proto_basic_resource() {
        let mut schema = IndexMap::new();
        schema.insert(
            "id".to_string(),
            FieldSchema {
                primary: true,
                generated: true,
                ..field(FieldType::Uuid)
            },
        );
        schema.insert(
            "name".to_string(),
            FieldSchema {
                required: true,
                ..field(FieldType::String)
            },
        );
        schema.insert("active".to_string(), field(FieldType::Boolean));

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                filters: Some(vec!["active".to_string()]),
                search: Some(vec!["name".to_string()]),
                ..endpoint(HttpMethod::Get, "/items")
            },
        );
        endpoints.insert("get".to_string(), endpoint(HttpMethod::Get, "/items/:id"));
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                input: Some(vec!["name".to_string(), "active".to_string()]),
                ..endpoint(HttpMethod::Post, "/items")
            },
        );
        endpoints.insert(
            "delete".to_string(),
            endpoint(HttpMethod::Delete, "/items/:id"),
        );

        let resource = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        };

        let proto = generate_proto(&resource);

        assert!(proto.contains("syntax = \"proto3\";"));
        assert!(proto.contains("package shaperail.v1.items;"));
        assert!(proto.contains("message Item {"));
        assert!(proto.contains("string id = 1;"));
        assert!(proto.contains("string name = 2;"));
        assert!(proto.contains("bool active = 3;"));
        assert!(proto.contains("service ItemService {"));
        assert!(proto.contains("rpc ListItems(ListItemsRequest) returns (ListItemsResponse);"));
        assert!(proto.contains("rpc StreamItems(ListItemsRequest) returns (stream Item);"));
        assert!(proto.contains("rpc GetItem(GetItemRequest) returns (GetItemResponse);"));
        assert!(proto.contains("rpc CreateItem(CreateItemRequest) returns (CreateItemResponse);"));
        assert!(proto.contains("rpc DeleteItem(DeleteItemRequest) returns (DeleteItemResponse);"));
        assert!(proto.contains("string active = 1;"));
        assert!(proto.contains("string search = 2;"));
        assert!(proto.contains("string cursor = 3;"));
    }

    #[test]
    fn generate_proto_with_timestamp() {
        let mut schema = IndexMap::new();
        schema.insert(
            "id".to_string(),
            FieldSchema {
                primary: true,
                generated: true,
                ..field(FieldType::Uuid)
            },
        );
        schema.insert(
            "created_at".to_string(),
            FieldSchema {
                generated: true,
                ..field(FieldType::Timestamp)
            },
        );

        let resource = ResourceDefinition {
            resource: "events".to_string(),
            version: 2,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        };

        let proto = generate_proto(&resource);
        assert!(proto.contains("import \"google/protobuf/timestamp.proto\";"));
        assert!(proto.contains("google.protobuf.Timestamp created_at = 2;"));
        assert!(proto.contains("package shaperail.v2.events;"));
    }

    #[test]
    fn generate_all_protos_multiple() {
        let make_schema = || {
            let mut s = IndexMap::new();
            s.insert(
                "id".to_string(),
                FieldSchema {
                    primary: true,
                    ..field(FieldType::Uuid)
                },
            );
            s
        };

        let resources = vec![
            ResourceDefinition {
                resource: "users".to_string(),
                version: 1,
                db: None,
                tenant_key: None,
                schema: make_schema(),
                endpoints: None,
                relations: None,
                indexes: None,
            },
            ResourceDefinition {
                resource: "orders".to_string(),
                version: 1,
                db: None,
                tenant_key: None,
                schema: make_schema(),
                endpoints: None,
                relations: None,
                indexes: None,
            },
        ];

        let protos = generate_all_protos(&resources);
        assert_eq!(protos.len(), 2);
        assert_eq!(protos[0].0, "users.proto");
        assert_eq!(protos[1].0, "orders.proto");
        assert!(protos[0].1.contains("message User {"));
        assert!(protos[1].1.contains("message Order {"));
    }
}
