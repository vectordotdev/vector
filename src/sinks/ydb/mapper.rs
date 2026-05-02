use chrono::TimeZone;
use snafu::Snafu;
use tracing::warn;
use vector_lib::event::{Event, Value as VectorValue};
use ydb::YdbDecimal;
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
            _ => {
                return Err(MappingError::VectorCommon {
                    source: "Only Log and Trace events are supported".into(),
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

    match (vector_val, &inner_type) {
        (VectorValue::Integer(i), Value::Int64(_)) => Ok(Value::Int64(*i)),

        (VectorValue::Float(f), Value::Double(_)) => Ok(Value::Double(f.into_inner())),
        (VectorValue::Float(f), Value::Decimal(ydb_decimal)) => {
            let decimal_val = decimal_rs::Decimal::try_from(f.into_inner()).map_err(|e| {
                MappingError::ConversionFailed {
                    message: format!("failed to convert Float to Decimal: {}", e),
                }
            })?;
            let ydb_decimal =
                YdbDecimal::try_new(decimal_val, ydb_decimal.precision(), ydb_decimal.scale())
                    .map_err(|e| MappingError::ConversionFailed {
                        message: format!("failed to create YdbDecimal: {}", e),
                    })?;
            Ok(Value::Decimal(ydb_decimal))
        }

        (VectorValue::Bytes(b), Value::Bytes(_)) => Ok(Value::Bytes(b.to_vec().into())),
        (VectorValue::Bytes(b), Value::Text(_)) => {
            let text =
                String::from_utf8(b.to_vec()).map_err(|_| MappingError::ConversionFailed {
                    message: "invalid UTF-8 in Bytes for Text field".to_string(),
                })?;
            Ok(Value::Text(text))
        }

        (VectorValue::Boolean(b), Value::Bool(_)) => Ok(Value::Bool(*b)),

        (VectorValue::Timestamp(ts), Value::Timestamp(_)) => {
            Ok(Value::Timestamp(std::time::SystemTime::from(*ts)))
        }
        (VectorValue::Timestamp(ts), Value::Date(_)) => {
            let date = ts.date_naive();
            let datetime =
                date.and_hms_opt(0, 0, 0)
                    .ok_or_else(|| MappingError::ConversionFailed {
                        message: "failed to create datetime".to_string(),
                    })?;
            let datetime_utc = chrono::Utc.from_utc_datetime(&datetime);
            Ok(Value::Date(std::time::SystemTime::from(datetime_utc)))
        }
        (VectorValue::Timestamp(ts), Value::DateTime(_)) => {
            Ok(Value::DateTime(std::time::SystemTime::from(*ts)))
        }

        (VectorValue::Array(_), Value::JsonDocument(_)) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Array".to_string(),
                    reason: e.to_string(),
                }
            })?;
            Ok(Value::JsonDocument(json_str))
        }
        (VectorValue::Array(_), Value::Json(_)) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Array".to_string(),
                    reason: e.to_string(),
                }
            })?;
            Ok(Value::Json(json_str))
        }

        (VectorValue::Object(_), Value::JsonDocument(_)) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Object".to_string(),
                    reason: e.to_string(),
                }
            })?;
            Ok(Value::JsonDocument(json_str))
        }
        (VectorValue::Object(_), Value::Json(_)) => {
            let json_str = serde_json::to_string(vector_val).map_err(|e| {
                MappingError::SerializationFailed {
                    what: "Object".to_string(),
                    reason: e.to_string(),
                }
            })?;
            Ok(Value::Json(json_str))
        }

        (VectorValue::Null, _) => Ok(Value::Null),

        (VectorValue::Regex(_), _) => Err(MappingError::ConversionFailed {
            message: "Regex type is not supported".to_string(),
        }),

        (val, typ) => Err(type_mismatch(val.to_string().as_str(), typ)),
    }
}
