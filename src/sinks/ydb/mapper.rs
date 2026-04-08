use chrono::TimeZone;
use decimal_rs::Decimal;
use snafu::Snafu;
use tracing::warn;
use vector_lib::event::{Event, Value as VectorValue};
use ydb::{TableDescription, Value};

#[derive(Debug, Snafu)]
pub enum MappingError {
    #[snafu(display("Event conversion error: {source}"))]
    VectorCommon { source: vector_common::Error },

    #[snafu(display("type mismatch: {} -> {}", vector_type, ydb_type))]
    TypeMismatch {
        vector_type: String,
        ydb_type: String,
    },

    #[snafu(display("failed to serialize {} to JSON: {}", what, reason))]
    SerializationFailed { what: String, reason: String },

    #[snafu(display("{}", message))]
    ConversionFailed { message: String },
}

pub struct EventMapper<'a> {
    schema: &'a TableDescription,
}

impl<'a> EventMapper<'a> {
    pub const fn new(schema: &'a TableDescription) -> Self {
        Self { schema }
    }

    pub fn map_event(&self, event: Event) -> Result<Value, MappingError> {
        let event_map = match &event {
            Event::Log(log) => log.as_map().ok_or_else(|| MappingError::VectorCommon {
                source: "Log event is not an object/map".into(),
            })?,
            Event::Trace(trace) => {
                trace
                    .value()
                    .as_object()
                    .ok_or_else(|| MappingError::VectorCommon {
                        source: "Trace event is not an object/map".into(),
                    })?
            }
            Event::Metric(_) => {
                // TODO: cover metrics
                return Err(MappingError::VectorCommon {
                    source: "Metric events are not yet supported, only Log and Trace".into(),
                });
            }
        };

        let mut fields = Vec::new();

        for column in &self.schema.columns {
            if let Some(vector_value) = event_map.get(column.name.as_str()) {
                let type_example = match &column.type_value {
                    Ok(val) => val,
                    Err(e) => {
                        warn!(
                            message = "Skipping column with unsupported type",
                            column = %column.name,
                            error = %e.error,
                        );
                        continue;
                    }
                };

                let ydb_val = match convert_value(vector_value, type_example) {
                    Ok(val) => val,
                    Err(e) => {
                        warn!(
                            message = "Failed to convert field, skipping",
                            column = %column.name,
                            error = %e.to_string(),
                        );
                        continue;
                    }
                };

                fields.push((column.name.clone(), ydb_val));
            }
        }

        Ok(Value::struct_from_fields(fields))
    }
}

fn type_mismatch(vector_type: &str, ydb_type: &Value) -> MappingError {
    MappingError::TypeMismatch {
        vector_type: vector_type.to_string(),
        ydb_type: format!("{:?}", ydb_type),
    }
}

fn convert_value(vector_val: &VectorValue, ydb_type_hint: &Value) -> Result<Value, MappingError> {
    let inner_type = if let Value::Optional(_) = ydb_type_hint {
        ydb_type_hint
            .clone()
            .to_option()
            .ok_or_else(|| MappingError::ConversionFailed {
                message: "Cannot extract inner type from Optional".to_string(),
            })?
    } else {
        ydb_type_hint.clone()
    };

    match vector_val {
        VectorValue::Integer(i) => match &inner_type {
            Value::Int64(_) => Ok(Value::Int64(*i)),
            _ => Err(type_mismatch("Integer", &inner_type)),
        },

        VectorValue::Float(f) => match &inner_type {
            Value::Double(_) => Ok(Value::Double(f.into_inner())),
            Value::Decimal(_) => Ok(Value::Decimal(Decimal::try_from(f.into_inner()).map_err(
                |e| MappingError::ConversionFailed {
                    message: format!("failed to convert Float to Decimal: {}", e),
                },
            )?)),
            _ => Err(type_mismatch("Float", &inner_type)),
        },

        VectorValue::Bytes(b) => match &inner_type {
            Value::Bytes(_) => Ok(Value::Bytes(b.to_vec().into())),
            Value::Text(_) => {
                let text =
                    String::from_utf8(b.to_vec()).map_err(|_| MappingError::ConversionFailed {
                        message: "invalid UTF-8 in Bytes for Text field".to_string(),
                    })?;
                Ok(Value::Text(text))
            }
            _ => Err(type_mismatch("Bytes", &inner_type)),
        },

        VectorValue::Boolean(b) => match &inner_type {
            Value::Bool(_) => Ok(Value::Bool(*b)),
            _ => Err(type_mismatch("Boolean", &inner_type)),
        },

        VectorValue::Timestamp(ts) => match &inner_type {
            Value::Timestamp(_) => Ok(Value::Timestamp(std::time::SystemTime::from(*ts))),
            Value::Date(_) => {
                let date = ts.date_naive();
                let datetime =
                    date.and_hms_opt(0, 0, 0)
                        .ok_or_else(|| MappingError::ConversionFailed {
                            message: "failed to create datetime".to_string(),
                        })?;
                let datetime_utc = chrono::Utc.from_utc_datetime(&datetime);
                Ok(Value::Date(std::time::SystemTime::from(datetime_utc)))
            }
            Value::DateTime(_) => Ok(Value::DateTime(std::time::SystemTime::from(*ts))),
            _ => Err(type_mismatch("Timestamp", &inner_type)),
        },

        VectorValue::Array(_) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Array".to_string(),
                    reason: e.to_string(),
                }
            })?;

            match &inner_type {
                Value::JsonDocument(_) => Ok(Value::JsonDocument(json_str)),
                Value::Json(_) => Ok(Value::Json(json_str)),
                _ => Err(type_mismatch("Array", &inner_type)),
            }
        }

        VectorValue::Object(_) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Object".to_string(),
                    reason: e.to_string(),
                }
            })?;

            match &inner_type {
                Value::JsonDocument(_) => Ok(Value::JsonDocument(json_str)),
                Value::Json(_) => Ok(Value::Json(json_str)),
                _ => Err(type_mismatch("Object", &inner_type)),
            }
        }

        VectorValue::Null => Ok(Value::Null),

        // TODO: add support for Regex type later
        VectorValue::Regex(_) => Err(MappingError::ConversionFailed {
            message: "Regex type is not supported".to_string(),
        }),
    }
}
