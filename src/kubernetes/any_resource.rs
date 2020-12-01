//! Arbitrary Kubernetes resource container.

use super::resource_version;
use serde::Deserialize;
use serde_json::{Map, Value};
use snafu::Snafu;
use std::convert::TryFrom;

/// A container for an arbitrary Kubernetes resource, represented as
/// a JSON object.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AnyResource {
    #[serde(flatten)]
    data: Map<String, Value>,
}

impl AsRef<Map<String, Value>> for AnyResource {
    fn as_ref(&self) -> &Map<String, Value> {
        &self.data
    }
}

impl From<Map<String, Value>> for AnyResource {
    fn from(data: Map<String, Value>) -> Self {
        Self { data }
    }
}

impl TryFrom<Value> for AnyResource {
    type Error = TryFromValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let data = match value {
            Value::Object(data) => data,
            _ => return Err(TryFromValueError::NotAnObject { value }),
        };
        Ok(data.into())
    }
}

/// An error that can occur when converting an abitrary JSON value into
/// an [`AnyResource`].
#[derive(Debug, PartialEq, Snafu)]
pub enum TryFromValueError {
    /// Provided value was not an object.
    NotAnObject {
        /// The input value that was passed.
        value: Value,
    },
}

impl resource_version::Resource for AnyResource {
    /// Obtain a resource version by taking a `metadata.resource_version` value
    /// of the underlying JSON object.
    fn resource_version(&self) -> Option<resource_version::Candidate> {
        let maybe_value = self
            .data
            .get("metadata")
            .and_then(|metadata| metadata.get("resource_version"));

        let value = match maybe_value {
            Some(val) => val,
            None => {
                warn!(message = "Got a resource without a `metadata.resource_version`.");
                return None;
            }
        };

        let new_resource_version = match value {
            Value::String(ref val) => val,
            _ => {
                warn!(
                    message = "Got a resource where a `metadata.resource_version` is not a string."
                );
                return None;
            }
        };

        Some(resource_version::Candidate::new_unchecked(
            new_resource_version.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kubernetes::resource_version::Resource;
    use serde_json::json;

    #[test]
    fn test_deserialize() {
        let cases = vec![
            // Valid.
            (
                r#"{
                    "apiVersion": "v1",
                    "kind": "Pod",
                    "metadata": {
                        "name": "pod-name"
                    }
                }"#,
                Some(json!({
                    "apiVersion": "v1",
                    "kind": "Pod",
                    "metadata": {
                        "name": "pod-name"
                    }
                })),
            ),
            // Valid - edge case - empty object.
            (r#"{}"#, Some(json!({}))),
            // Invalid - incomplete object.
            (r#"{"#, None),
            // Invalid - not an object - number.
            (r#"123"#, None),
        ];

        for (input, expected_value) in cases {
            let expected = expected_value.map(|value| {
                AnyResource::try_from(value).expect("invalid test case expected sample")
            });

            let resource = serde_json::from_str::<AnyResource>(input);
            let actual = resource.ok();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_resource_version() {
        let cases = vec![
            // Valid.
            (
                json!({
                    "metadata": {
                        "resource_version": "12345"
                    }
                }),
                Some("12345"),
            ),
            // Valid - empty string edge case.
            (
                json!({
                    "metadata": {
                        "resource_version": ""
                    }
                }),
                Some(""),
            ),
            // Invalid - no metadata.
            (json!({}), None),
            // Invalid - no resource version.
            (json!({"metadata": {}}), None),
            // Invalid - resource version not a string.
            (
                json!({
                    "metadata": {
                        "resource_version": null
                    }
                }),
                None,
            ),
        ];

        for (input, expected) in cases {
            let expected =
                expected.map(|val| resource_version::Candidate::new_unchecked(val.to_owned()));

            let resource = AnyResource::try_from(input).expect("invalid test case input");
            assert_eq!(resource.resource_version(), expected);
        }
    }
}
