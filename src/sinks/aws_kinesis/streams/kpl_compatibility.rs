//! Check the distribution strategy before starting a KPL producer.
//!
//! Inspect the buffered response after deserialization to distinguish an omitted
//! strategy from explicit null. The SDK maps both cases to an absent option.

use aws_smithy_runtime_api::{
    box_error::BoxError,
    client::{
        interceptors::{Intercept, context::AfterDeserializationInterceptorContextRef},
        runtime_components::RuntimeComponents,
    },
};
use aws_smithy_types::config_bag::ConfigBag;

#[derive(Debug)]
pub(super) struct KplCompatibility;

impl Intercept for KplCompatibility {
    fn name(&self) -> &'static str {
        "KplCompatibility"
    }

    fn read_after_deserialization(
        &self,
        context: &AfterDeserializationInterceptorContextRef<'_>,
        _: &RuntimeComponents,
        _: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if !context.response().status().is_success() {
            return Ok(());
        }
        let body = context
            .response()
            .body()
            .bytes()
            .ok_or("KPL compatibility check requires a buffered stream summary")?;
        validate(body)
    }
}

fn validate(body: &[u8]) -> Result<(), BoxError> {
    let response: serde_json::Value = serde_json::from_slice(body)?;
    let summary = response
        .get("StreamDescriptionSummary")
        .and_then(serde_json::Value::as_object)
        .ok_or("KPL compatibility check requires StreamDescriptionSummary")?;
    match summary.get("RecordDistributionStrategy") {
        // Provisioned streams and older endpoints omit this field.
        None => Ok(()),
        Some(serde_json::Value::String(strategy)) if strategy == "USER_PARTITION_KEY" => Ok(()),
        _ => Err("KPL aggregation requires USER_PARTITION_KEY distribution; AUTO or unknown distribution can cause consumer data loss".into()),
    }
}
