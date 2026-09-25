use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent},
};
use vrl::path::OwnedTargetPath;

#[derive(Debug, NamedInternalEvent)]
pub struct DatadogLogsEventTruncated {
    pub max_payload_bytes: usize,
    pub original_estimated_size: usize,
}

impl InternalEvent for DatadogLogsEventTruncated {
    fn emit(self) {
        warn!(
            message = "Truncated Datadog log event that exceeded the maximum payload size.",
            max_payload_bytes = self.max_payload_bytes,
            original_estimated_size = self.original_estimated_size,
        );
        counter!(CounterName::DatadogLogsEventsTruncatedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct DatadogLogsReservedAttributeConflict<'a> {
    pub meaning: &'static str,
    pub source_path: &'a OwnedTargetPath,
    pub destination_path: &'a str,
    pub renamed_existing_to: &'a str,
}

impl InternalEvent for DatadogLogsReservedAttributeConflict<'_> {
    fn emit(self) {
        warn!(
            message = "Relocated a field with semantic meaning to a Datadog reserved attribute, but the destination path already exists. The existing field was renamed to not overwrite.",
            meaning = self.meaning,
            source_path = %self.source_path,
            destination_path = self.destination_path,
            renamed_existing_to = self.renamed_existing_to,
        );
        counter!(
            CounterName::DatadogLogsReservedAttributeConflictsTotal,
            "meaning" => self.meaning,
        )
        .increment(1);
    }
}
