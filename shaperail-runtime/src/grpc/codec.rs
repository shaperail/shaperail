//! Protobuf encoding/decoding helpers for dynamic resource messages.
//!
//! Maps between `serde_json::Value` (used internally) and protobuf wire format
//! using `prost` primitives. Field numbers are assigned based on schema order.

use prost::bytes::{BufMut, BytesMut};
use shaperail_core::FieldType;

/// Protobuf wire types.
const WIRE_VARINT: u32 = 0;
const WIRE_LEN: u32 = 2;
const WIRE_64BIT: u32 = 1;

/// Encode a field tag (field_number << 3 | wire_type).
fn encode_tag(field_number: u32, wire_type: u32) -> u32 {
    (field_number << 3) | wire_type
}

/// Encode a varint into the buffer.
fn encode_varint(buf: &mut BytesMut, mut value: u64) {
    loop {
        if value < 0x80 {
            buf.put_u8(value as u8);
            return;
        }
        buf.put_u8((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
}

/// Encode a single JSON value as a protobuf field.
pub fn encode_field(
    buf: &mut BytesMut,
    field_number: u32,
    field_type: &FieldType,
    value: &serde_json::Value,
) {
    match (field_type, value) {
        (_, serde_json::Value::Null) => {} // skip null fields
        (FieldType::Integer, serde_json::Value::Number(n)) => {
            if let Some(v) = n.as_i64() {
                encode_varint(buf, encode_tag(field_number, WIRE_VARINT) as u64);
                // int32 uses zigzag encoding? No, regular varint for int32 in proto3
                encode_varint(buf, v as u64);
            }
        }
        (FieldType::Number, serde_json::Value::Number(n)) => {
            if let Some(v) = n.as_f64() {
                encode_varint(buf, encode_tag(field_number, WIRE_64BIT) as u64);
                buf.put_f64_le(v);
            }
        }
        (FieldType::Boolean, serde_json::Value::Bool(b)) => {
            encode_varint(buf, encode_tag(field_number, WIRE_VARINT) as u64);
            encode_varint(buf, if *b { 1 } else { 0 });
        }
        // String-like types: uuid, string, date, enum, file, timestamp (as RFC3339)
        (_, serde_json::Value::String(s)) => {
            encode_varint(buf, encode_tag(field_number, WIRE_LEN) as u64);
            encode_varint(buf, s.len() as u64);
            buf.put_slice(s.as_bytes());
        }
        // JSON/Array: serialize as JSON string
        (FieldType::Json | FieldType::Array, _) => {
            let s = serde_json::to_string(value).unwrap_or_default();
            encode_varint(buf, encode_tag(field_number, WIRE_LEN) as u64);
            encode_varint(buf, s.len() as u64);
            buf.put_slice(s.as_bytes());
        }
        // Fallback: encode as string
        (_, v) => {
            let s = v.to_string();
            encode_varint(buf, encode_tag(field_number, WIRE_LEN) as u64);
            encode_varint(buf, s.len() as u64);
            buf.put_slice(s.as_bytes());
        }
    }
}

/// Encode a JSON object as a protobuf message using the resource schema field order.
pub fn encode_resource_message(
    schema: &indexmap::IndexMap<String, shaperail_core::FieldSchema>,
    record: &serde_json::Value,
) -> prost::bytes::Bytes {
    let mut buf = BytesMut::new();
    if let serde_json::Value::Object(obj) = record {
        for (i, (field_name, field_schema)) in schema.iter().enumerate() {
            let field_number = (i + 1) as u32;
            if let Some(value) = obj.get(field_name) {
                encode_field(&mut buf, field_number, &field_schema.field_type, value);
            }
        }
    }
    buf.freeze()
}

/// Decode a protobuf length-delimited string field from raw bytes at the given position.
/// Returns (value, bytes_consumed).
fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Decode a protobuf message into a JSON object using the resource schema.
pub fn decode_resource_message(
    schema: &indexmap::IndexMap<String, shaperail_core::FieldSchema>,
    data: &[u8],
) -> serde_json::Value {
    let field_names: Vec<&str> = schema.keys().map(|k| k.as_str()).collect();
    let field_types: Vec<&FieldType> = schema.values().map(|v| &v.field_type).collect();
    let mut obj = serde_json::Map::new();
    let mut pos = 0;

    while pos < data.len() {
        let (tag, consumed) = match decode_varint(&data[pos..]) {
            Some(v) => v,
            None => break,
        };
        pos += consumed;

        let field_number = (tag >> 3) as usize;
        let wire_type = (tag & 0x7) as u32;

        if field_number == 0 || field_number > field_names.len() {
            // Skip unknown field
            match wire_type {
                WIRE_VARINT => {
                    if let Some((_, c)) = decode_varint(&data[pos..]) {
                        pos += c;
                    } else {
                        break;
                    }
                }
                WIRE_64BIT => pos += 8,
                WIRE_LEN => {
                    if let Some((len, c)) = decode_varint(&data[pos..]) {
                        pos += c + len as usize;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
            continue;
        }

        let idx = field_number - 1;
        let name = field_names[idx];
        let ft = field_types[idx];

        match wire_type {
            WIRE_VARINT => {
                if let Some((val, c)) = decode_varint(&data[pos..]) {
                    pos += c;
                    match ft {
                        FieldType::Boolean => {
                            obj.insert(name.to_string(), serde_json::Value::Bool(val != 0));
                        }
                        FieldType::Integer => {
                            obj.insert(
                                name.to_string(),
                                serde_json::Value::Number(serde_json::Number::from(val as i64)),
                            );
                        }
                        _ => {
                            obj.insert(
                                name.to_string(),
                                serde_json::Value::Number(serde_json::Number::from(val as i64)),
                            );
                        }
                    }
                } else {
                    break;
                }
            }
            WIRE_64BIT => {
                if pos + 8 <= data.len() {
                    let bytes: [u8; 8] = data[pos..pos + 8].try_into().unwrap_or([0; 8]);
                    let val = f64::from_le_bytes(bytes);
                    pos += 8;
                    if let Some(n) = serde_json::Number::from_f64(val) {
                        obj.insert(name.to_string(), serde_json::Value::Number(n));
                    }
                } else {
                    break;
                }
            }
            WIRE_LEN => {
                if let Some((len, c)) = decode_varint(&data[pos..]) {
                    pos += c;
                    let end = pos + len as usize;
                    if end <= data.len() {
                        let bytes = &data[pos..end];
                        pos = end;
                        let s = String::from_utf8_lossy(bytes).to_string();
                        match ft {
                            FieldType::Json | FieldType::Array => {
                                if let Ok(parsed) = serde_json::from_str(&s) {
                                    obj.insert(name.to_string(), parsed);
                                } else {
                                    obj.insert(name.to_string(), serde_json::Value::String(s));
                                }
                            }
                            _ => {
                                obj.insert(name.to_string(), serde_json::Value::String(s));
                            }
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use shaperail_core::FieldSchema;

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

    #[test]
    fn roundtrip_string_field() {
        let mut schema = IndexMap::new();
        schema.insert("name".to_string(), field(FieldType::String));

        let record = serde_json::json!({"name": "Alice"});
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        assert_eq!(decoded["name"], "Alice");
    }

    #[test]
    fn roundtrip_integer_field() {
        let mut schema = IndexMap::new();
        schema.insert("count".to_string(), field(FieldType::Integer));

        let record = serde_json::json!({"count": 42});
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        assert_eq!(decoded["count"], 42);
    }

    #[test]
    fn roundtrip_boolean_field() {
        let mut schema = IndexMap::new();
        schema.insert("active".to_string(), field(FieldType::Boolean));

        let record = serde_json::json!({"active": true});
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        assert_eq!(decoded["active"], true);
    }

    #[test]
    fn roundtrip_multiple_fields() {
        let mut schema = IndexMap::new();
        schema.insert("id".to_string(), field(FieldType::Uuid));
        schema.insert("name".to_string(), field(FieldType::String));
        schema.insert("age".to_string(), field(FieldType::Integer));
        schema.insert("active".to_string(), field(FieldType::Boolean));

        let record = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Bob",
            "age": 30,
            "active": false
        });
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        assert_eq!(decoded["id"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(decoded["name"], "Bob");
        assert_eq!(decoded["age"], 30);
        assert_eq!(decoded["active"], false);
    }

    #[test]
    fn null_fields_skipped() {
        let mut schema = IndexMap::new();
        schema.insert("name".to_string(), field(FieldType::String));
        schema.insert("bio".to_string(), field(FieldType::String));

        let record = serde_json::json!({"name": "Eve", "bio": null});
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        assert_eq!(decoded["name"], "Eve");
        assert!(decoded.get("bio").is_none());
    }

    #[test]
    fn double_field_roundtrip() {
        let mut schema = IndexMap::new();
        schema.insert("price".to_string(), field(FieldType::Number));

        let record = serde_json::json!({"price": 19.99});
        let encoded = encode_resource_message(&schema, &record);
        let decoded = decode_resource_message(&schema, &encoded);
        let price = decoded["price"].as_f64().unwrap();
        assert!((price - 19.99).abs() < 0.001);
    }
}
