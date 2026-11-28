use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("expected JSON object for tool arguments")]
    ExpectedObject,
    #[error("missing required field: {0}")]
    MissingRequired(String),
    #[error("field '{field}' expected type '{expected}'")]
    TypeMismatch { field: String, expected: String },
}

/// Validates tool call arguments against a JSON Schema `object` definition.
pub fn validate_object_args(schema: &Value, args: &Value) -> Result<(), SchemaError> {
    let args_obj = args.as_object().ok_or(SchemaError::ExpectedObject)?;
    let Some(schema_obj) = schema.as_object() else {
        return Ok(());
    };

    if let Some(required) = schema_obj.get("required").and_then(Value::as_array) {
        for req in required {
            if let Some(key) = req.as_str()
                && !args_obj.contains_key(key)
            {
                return Err(SchemaError::MissingRequired(key.to_string()));
            }
        }
    }

    if let Some(props) = schema_obj.get("properties").and_then(Value::as_object) {
        for (key, prop_schema) in props {
            if let Some(val) = args_obj.get(key)
                && let Some(expected) = prop_schema.get("type").and_then(Value::as_str)
            {
                let valid = match expected {
                    "string" => val.is_string(),
                    "integer" => val.is_i64() || val.is_u64(),
                    "number" => val.is_number(),
                    "boolean" => val.is_boolean(),
                    "array" => val.is_array(),
                    "object" => val.is_object(),
                    _ => true,
                };
                if !valid {
                    return Err(SchemaError::TypeMismatch {
                        field: key.clone(),
                        expected: expected.to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_required_and_types() {
        let schema = json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "limit": { "type": "integer" }
            }
        });
        assert_eq!(validate_object_args(&schema, &json!({"path": "a.rs", "limit": 10})), Ok(()));
        assert_eq!(
            validate_object_args(&schema, &json!({"limit": 10})),
            Err(SchemaError::MissingRequired("path".into()))
        );
        assert_eq!(
            validate_object_args(&schema, &json!({"path": 123})),
            Err(SchemaError::TypeMismatch {
                field: "path".into(),
                expected: "string".into()
            })
        );
    }

    #[test]
    fn rejects_non_object_and_checks_boolean_array_object_types() {
        let schema = json!({
            "type": "object",
            "properties": {
                "flag": { "type": "boolean" },
                "items": { "type": "array" },
                "meta": { "type": "object" }
            }
        });
        assert_eq!(
            validate_object_args(&schema, &json!(["not-an-object"])),
            Err(SchemaError::ExpectedObject)
        );
        assert_eq!(
            validate_object_args(&schema, &json!({"flag": "yes"})),
            Err(SchemaError::TypeMismatch {
                field: "flag".into(),
                expected: "boolean".into()
            })
        );
        assert_eq!(
            validate_object_args(&schema, &json!({"flag": true, "items": [1], "meta": {}})),
            Ok(())
        );
    }
}
