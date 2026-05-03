//! MongoDB backend (M14). Provides CRUD operations via the mongodb driver.
//!
//! Resources routed to a MongoDB connection use BSON documents instead of SQL rows.
//! Schema validation is applied via MongoDB's JSON Schema validator on collection creation.

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, FindOptions};
use mongodb::Client;
use serde_json::{Map, Value};
use std::sync::Arc;

use shaperail_core::{FieldSchema, FieldType, ResourceDefinition, ShaperailError};

use super::filter::FilterSet;
use super::pagination::PageRequest;
use super::search::SearchParam;
use super::sort::SortParam;
use super::store::ResourceStore;
use super::ResourceRow;

/// A MongoDB connection wrapping a database handle.
#[derive(Clone)]
pub struct MongoConnection {
    pub db: mongodb::Database,
    pub client: Arc<Client>,
}

impl MongoConnection {
    /// Connect to MongoDB from a URL. The database name is extracted from the URL path.
    pub async fn connect(url: &str) -> Result<Self, ShaperailError> {
        let opts = ClientOptions::parse(url)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to parse MongoDB URL: {e}")))?;
        let db_name = opts
            .default_database
            .clone()
            .unwrap_or_else(|| "shaperail".to_string());
        let client = Client::with_options(opts).map_err(|e| {
            ShaperailError::Internal(format!("Failed to create MongoDB client: {e}"))
        })?;
        let db = client.database(&db_name);
        // Verify connectivity.
        db.run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to connect to MongoDB: {e}")))?;
        tracing::info!("Connected to MongoDB database '{db_name}'");
        Ok(Self {
            db,
            client: Arc::new(client),
        })
    }

    /// Ensure the collection exists with JSON Schema validation from the resource definition.
    pub async fn ensure_collection(
        &self,
        resource: &ResourceDefinition,
    ) -> Result<(), ShaperailError> {
        let collection_name = &resource.resource;
        let schema = build_json_schema(resource);

        // Try to create collection with validator; ignore "already exists" errors.
        let create_opts = doc! {
            "create": collection_name,
            "validator": {
                "$jsonSchema": schema.clone()
            }
        };
        match self.db.run_command(create_opts).await {
            Ok(_) => {
                tracing::info!(
                    "Created MongoDB collection '{collection_name}' with schema validation"
                );
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("already exists") {
                    // Update validator on existing collection.
                    let modify = doc! {
                        "collMod": collection_name,
                        "validator": {
                            "$jsonSchema": schema
                        }
                    };
                    self.db.run_command(modify).await.map_err(|e| {
                        ShaperailError::Internal(format!(
                            "Failed to update MongoDB schema for '{collection_name}': {e}"
                        ))
                    })?;
                } else {
                    return Err(ShaperailError::Internal(format!(
                        "Failed to create MongoDB collection '{collection_name}': {e}"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Build a MongoDB JSON Schema document from a resource definition.
fn build_json_schema(resource: &ResourceDefinition) -> Document {
    let mut properties = Document::new();
    let mut required = Vec::new();

    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let bson_type = field_type_to_bson_type(&field.field_type);
        let mut prop = doc! { "bsonType": bson_type };

        if let Some(values) = &field.values {
            let bson_values: Vec<Bson> = values.iter().map(|v| Bson::String(v.clone())).collect();
            prop.insert("enum", bson_values);
        }
        if let Some(min) = &field.min {
            if let Some(n) = min.as_i64().or_else(|| min.as_f64().map(|f| f as i64)) {
                if matches!(
                    field.field_type,
                    FieldType::String | FieldType::Enum | FieldType::File
                ) {
                    prop.insert("minLength", n);
                } else {
                    prop.insert("minimum", n);
                }
            }
        }
        if let Some(max) = &field.max {
            if let Some(n) = max.as_i64().or_else(|| max.as_f64().map(|f| f as i64)) {
                if matches!(
                    field.field_type,
                    FieldType::String | FieldType::Enum | FieldType::File
                ) {
                    prop.insert("maxLength", n);
                } else {
                    prop.insert("maximum", n);
                }
            }
        }
        properties.insert(name.clone(), prop);

        if field.required && !field.generated {
            required.push(Bson::String(name.clone()));
        }
    }

    let mut schema = doc! {
        "bsonType": "object",
        "properties": properties,
    };
    if !required.is_empty() {
        schema.insert("required", required);
    }
    schema
}

fn field_type_to_bson_type(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Uuid | FieldType::String | FieldType::Enum | FieldType::File => "string",
        FieldType::Integer => "long",
        FieldType::Number => "double",
        FieldType::Boolean => "bool",
        FieldType::Timestamp | FieldType::Date => "string",
        FieldType::Json => "object",
        FieldType::Array => "array",
    }
}

/// Convert a JSON value to a BSON value for a given field schema.
fn json_to_bson(value: &Value, field: &FieldSchema) -> Bson {
    if value.is_null() {
        return Bson::Null;
    }
    match field.field_type {
        FieldType::Uuid | FieldType::String | FieldType::Enum | FieldType::File => {
            Bson::String(value.as_str().unwrap_or(&value.to_string()).to_string())
        }
        FieldType::Integer => Bson::Int64(value.as_i64().unwrap_or(0)),
        FieldType::Number => Bson::Double(value.as_f64().unwrap_or(0.0)),
        FieldType::Boolean => Bson::Boolean(value.as_bool().unwrap_or(false)),
        FieldType::Timestamp | FieldType::Date => {
            Bson::String(value.as_str().unwrap_or(&value.to_string()).to_string())
        }
        FieldType::Json => mongodb::bson::to_bson(value).unwrap_or(Bson::Null),
        FieldType::Array => {
            if let Some(arr) = value.as_array() {
                Bson::Array(
                    arr.iter()
                        .map(|v| mongodb::bson::to_bson(v).unwrap_or(Bson::Null))
                        .collect(),
                )
            } else {
                Bson::Null
            }
        }
    }
}

/// Convert a BSON value to JSON for a given field type.
fn bson_to_json(bson: &Bson, _field: &FieldSchema) -> Value {
    match bson {
        Bson::Null => Value::Null,
        Bson::String(s) => Value::String(s.clone()),
        Bson::Int32(n) => Value::Number((*n as i64).into()),
        Bson::Int64(n) => Value::Number((*n).into()),
        Bson::Double(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Bson::Boolean(b) => Value::Bool(*b),
        Bson::Document(d) => {
            mongodb::bson::from_document::<Value>(d.clone()).unwrap_or(Value::Null)
        }
        Bson::Array(arr) => Value::Array(arr.iter().map(|b| bson_to_json(b, _field)).collect()),
        _ => Value::String(bson.to_string()),
    }
}

/// Convert a BSON document to a JSON object using the resource schema.
fn doc_to_json(doc: &Document, resource: &ResourceDefinition) -> Value {
    let mut obj = Map::new();
    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let bson_val = doc.get(name).unwrap_or(&Bson::Null);
        obj.insert(name.clone(), bson_to_json(bson_val, field));
    }
    Value::Object(obj)
}

/// MongoDB-backed resource store (M14).
pub struct MongoBackedStore {
    resource: Arc<ResourceDefinition>,
    connection: MongoConnection,
}

impl MongoBackedStore {
    pub fn new(resource: Arc<ResourceDefinition>, connection: MongoConnection) -> Self {
        Self {
            resource,
            connection,
        }
    }

    fn collection(&self) -> mongodb::Collection<Document> {
        self.connection.db.collection(&self.resource.resource)
    }

    fn primary_key(&self) -> &str {
        self.resource
            .schema
            .iter()
            .find(|(_, fs)| fs.primary)
            .map(|(name, _)| name.as_str())
            .unwrap_or("id")
    }

    fn has_soft_delete(&self) -> bool {
        self.resource
            .endpoints
            .as_ref()
            .map(|eps| eps.values().any(|ep| ep.soft_delete))
            .unwrap_or(false)
    }

    fn not_deleted_filter(&self) -> Document {
        if self.has_soft_delete() {
            doc! { "deleted_at": Bson::Null }
        } else {
            doc! {}
        }
    }
}

#[async_trait::async_trait]
impl ResourceStore for MongoBackedStore {
    fn resource_name(&self) -> &str {
        &self.resource.resource
    }

    async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let mut filter = self.not_deleted_filter();
        filter.insert(pk, id.to_string());

        let result = self
            .collection()
            .find_one(filter)
            .await
            .map_err(|e| ShaperailError::Internal(format!("MongoDB find_by_id failed: {e}")))?;

        match result {
            Some(doc) => {
                let json = doc_to_json(&doc, &self.resource);
                Ok(ResourceRow(json))
            }
            None => Err(ShaperailError::NotFound),
        }
    }

    async fn find_all(
        &self,
        _endpoint: &shaperail_core::EndpointSpec,
        filters: &FilterSet,
        _search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, Value), ShaperailError> {
        let mut filter = self.not_deleted_filter();

        for f in &filters.filters {
            if let Some(field) = self.resource.schema.get(&f.field) {
                let bson_val = json_to_bson(&Value::String(f.value.clone()), field);
                filter.insert(f.field.clone(), bson_val);
            }
        }

        let mut sort_doc = Document::new();
        for s in &sort.fields {
            let dir = match s.direction {
                super::sort::SortDirection::Asc => 1,
                super::sort::SortDirection::Desc => -1,
            };
            sort_doc.insert(s.field.clone(), dir);
        }

        match page {
            PageRequest::Cursor { after, limit } => {
                if let Some(cursor) = after {
                    let id_str = super::pagination::decode_cursor(cursor)?;
                    let pk = self.primary_key();
                    filter.insert(pk, doc! { "$gt": id_str });
                }
                if sort_doc.is_empty() {
                    sort_doc.insert(self.primary_key(), 1);
                }
                let opts = FindOptions::builder()
                    .sort(sort_doc)
                    .limit(Some(*limit + 1))
                    .build();
                let mut cursor = self
                    .collection()
                    .find(filter)
                    .with_options(opts)
                    .await
                    .map_err(|e| {
                        ShaperailError::Internal(format!("MongoDB find_all failed: {e}"))
                    })?;

                let mut rows = Vec::new();
                while cursor
                    .advance()
                    .await
                    .map_err(|e| ShaperailError::Internal(format!("MongoDB cursor error: {e}")))?
                {
                    let doc = cursor.deserialize_current().map_err(|e| {
                        ShaperailError::Internal(format!("MongoDB deserialize error: {e}"))
                    })?;
                    rows.push(doc);
                }

                let has_more = rows.len() as i64 > *limit;
                let result_rows: Vec<ResourceRow> = rows
                    .into_iter()
                    .take(*limit as usize)
                    .map(|d| ResourceRow(doc_to_json(&d, &self.resource)))
                    .collect();
                let cursor_val = result_rows
                    .last()
                    .and_then(|r| r.0.get("id"))
                    .and_then(|v| v.as_str())
                    .map(super::pagination::encode_cursor);
                let meta = serde_json::json!({ "cursor": cursor_val, "has_more": has_more });
                Ok((result_rows, meta))
            }
            PageRequest::Offset { offset, limit } => {
                let opts = FindOptions::builder()
                    .sort(if sort_doc.is_empty() {
                        None
                    } else {
                        Some(sort_doc)
                    })
                    .skip(Some(*offset as u64))
                    .limit(Some(*limit))
                    .build();
                let mut cursor = self
                    .collection()
                    .find(filter)
                    .with_options(opts)
                    .await
                    .map_err(|e| {
                        ShaperailError::Internal(format!("MongoDB find_all failed: {e}"))
                    })?;

                let mut result_rows = Vec::new();
                while cursor
                    .advance()
                    .await
                    .map_err(|e| ShaperailError::Internal(format!("MongoDB cursor error: {e}")))?
                {
                    let doc = cursor.deserialize_current().map_err(|e| {
                        ShaperailError::Internal(format!("MongoDB deserialize error: {e}"))
                    })?;
                    result_rows.push(ResourceRow(doc_to_json(&doc, &self.resource)));
                }
                let total = result_rows.len() as i64;
                let meta = serde_json::json!({
                    "offset": offset,
                    "limit": limit,
                    "total": total
                });
                Ok((result_rows, meta))
            }
        }
    }

    async fn insert(&self, data: &Map<String, Value>) -> Result<ResourceRow, ShaperailError> {
        let mut doc = Document::new();
        let mut generated_id: Option<String> = None;

        for (name, field) in &self.resource.schema {
            if field.transient {
                continue;
            }
            if field.generated {
                match field.field_type {
                    FieldType::Uuid => {
                        let id = uuid::Uuid::new_v4().to_string();
                        doc.insert(name.clone(), Bson::String(id.clone()));
                        if field.primary {
                            generated_id = Some(id);
                        }
                    }
                    FieldType::Timestamp => {
                        doc.insert(name.clone(), Bson::String(chrono::Utc::now().to_rfc3339()));
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(value) = data.get(name) {
                doc.insert(name.clone(), json_to_bson(value, field));
            } else if let Some(default) = &field.default {
                doc.insert(name.clone(), json_to_bson(default, field));
            }
        }

        self.collection()
            .insert_one(&doc)
            .await
            .map_err(|e| ShaperailError::Internal(format!("MongoDB insert failed: {e}")))?;

        // Return the inserted document.
        if let Some(id_str) = generated_id {
            let pk = self.primary_key();
            let filter = doc! { pk: &id_str };
            let result = self.collection().find_one(filter).await.map_err(|e| {
                ShaperailError::Internal(format!("MongoDB find after insert failed: {e}"))
            })?;
            match result {
                Some(d) => Ok(ResourceRow(doc_to_json(&d, &self.resource))),
                None => Ok(ResourceRow(doc_to_json(&doc, &self.resource))),
            }
        } else {
            Ok(ResourceRow(doc_to_json(&doc, &self.resource)))
        }
    }

    async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &Map<String, Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let mut filter = self.not_deleted_filter();
        filter.insert(pk, id.to_string());

        let mut set_doc = Document::new();
        for (name, value) in data {
            if let Some(field) = self.resource.schema.get(name) {
                if field.primary || field.generated {
                    continue;
                }
                set_doc.insert(name.clone(), json_to_bson(value, field));
            }
        }
        if self.resource.schema.contains_key("updated_at") {
            set_doc.insert("updated_at", Bson::String(chrono::Utc::now().to_rfc3339()));
        }
        if set_doc.is_empty() {
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: "body".to_string(),
                    message: "No valid fields to update".to_string(),
                    code: "empty_update".to_string(),
                },
            ]));
        }

        let update = doc! { "$set": set_doc };
        let result = self
            .collection()
            .update_one(filter.clone(), update)
            .await
            .map_err(|e| ShaperailError::Internal(format!("MongoDB update failed: {e}")))?;

        if result.matched_count == 0 {
            return Err(ShaperailError::NotFound);
        }

        // Fetch updated document.
        let updated = self
            .collection()
            .find_one(filter)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("MongoDB find after update failed: {e}"))
            })?
            .ok_or(ShaperailError::NotFound)?;
        Ok(ResourceRow(doc_to_json(&updated, &self.resource)))
    }

    async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let mut filter = self.not_deleted_filter();
        filter.insert(pk, id.to_string());

        let update = doc! {
            "$set": {
                "deleted_at": chrono::Utc::now().to_rfc3339()
            }
        };
        let result = self
            .collection()
            .update_one(filter.clone(), update)
            .await
            .map_err(|e| ShaperailError::Internal(format!("MongoDB soft_delete failed: {e}")))?;

        if result.matched_count == 0 {
            return Err(ShaperailError::NotFound);
        }

        // Return with deleted_at set — remove not-deleted filter.
        let pk_filter = doc! { pk: id.to_string() };
        let doc = self
            .collection()
            .find_one(pk_filter)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("MongoDB find after soft_delete failed: {e}"))
            })?
            .ok_or(ShaperailError::NotFound)?;
        Ok(ResourceRow(doc_to_json(&doc, &self.resource)))
    }

    async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let row = self.find_by_id(id).await?;
        let pk = self.primary_key();
        let filter = doc! { pk: id.to_string() };

        self.collection()
            .delete_one(filter)
            .await
            .map_err(|e| ShaperailError::Internal(format!("MongoDB hard_delete failed: {e}")))?;

        Ok(row)
    }
}
