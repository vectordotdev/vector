use std::{io, io::Write};

use serde::Serialize;
use vector_lib::buffers::EventCount;
use vector_lib::{config::telemetry, event::Event, ByteSizeOf, EstimatedJsonEncodedSizeOf};
use vector_lib::{
    internal_event::TaggedEventsSent,
    json_size::JsonSize,
    request_metadata::{GetEventCountTags, GroupedCountByteSize},
};

use crate::{
    codecs::Transformer,
    event::{EventFinalizers, Finalizable, LogEvent},
    sinks::{
        elasticsearch::{BulkAction, VersionType},
        util::encoding::{as_tracked_write, Encoder},
    },
};

#[derive(Serialize)]
pub enum DocumentVersionType {
    External,
    ExternalGte,
}

impl DocumentVersionType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            DocumentVersionType::External => VersionType::External.as_str(),
            DocumentVersionType::ExternalGte => VersionType::ExternalGte.as_str(),
        }
    }
}

#[derive(Serialize)]
pub struct DocumentVersion {
    pub kind: DocumentVersionType,
    pub value: u64,
}

#[derive(Serialize)]
pub enum DocumentMetadata {
    WithoutId,
    Id(String),
    IdAndVersion(String, DocumentVersion),
}

#[derive(Serialize)]
pub struct ProcessedEvent {
    pub index: String,
    pub bulk_action: BulkAction,
    pub log: LogEvent,
    pub document_metadata: DocumentMetadata,
}

impl Finalizable for ProcessedEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.log.metadata_mut().take_finalizers()
    }
}

impl ByteSizeOf for ProcessedEvent {
    fn allocated_bytes(&self) -> usize {
        match &self.document_metadata {
            DocumentMetadata::WithoutId => {
                self.index.allocated_bytes() + self.log.allocated_bytes()
            }
            DocumentMetadata::Id(id) | DocumentMetadata::IdAndVersion(id, _) => {
                self.index.allocated_bytes() + self.log.allocated_bytes() + id.allocated_bytes()
            }
        }
    }
}

impl EstimatedJsonEncodedSizeOf for ProcessedEvent {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.log.estimated_json_encoded_size_of()
    }
}

impl EventCount for ProcessedEvent {
    fn event_count(&self) -> usize {
        // An Elasticsearch ProcessedEvent is mapped one-to-one with an event.
        1
    }
}

impl GetEventCountTags for ProcessedEvent {
    fn get_tags(&self) -> TaggedEventsSent {
        self.log.get_tags()
    }
}

#[derive(PartialEq, Eq, Default, Clone, Debug)]
pub struct ElasticsearchEncoder {
    pub transformer: Transformer,
    pub doc_type: String,
    pub suppress_type_name: bool,
}

impl Encoder<Vec<ProcessedEvent>> for ElasticsearchEncoder {
    fn encode_input(
        &self,
        input: Vec<ProcessedEvent>,
        writer: &mut dyn Write,
    ) -> std::io::Result<(usize, GroupedCountByteSize)> {
        let mut written_bytes = 0;
        let mut byte_size = telemetry().create_request_count_byte_size();
        for event in input {
            let log = {
                let mut event = Event::from(event.log);
                self.transformer.transform(&mut event);
                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                event.into_log()
            };
            written_bytes += write_bulk_action(
                writer,
                event.bulk_action.as_str(),
                &event.index,
                &self.doc_type,
                self.suppress_type_name,
                &event.document_metadata,
            )?;
            written_bytes +=
                as_tracked_write::<_, _, io::Error>(writer, &log, |mut writer, log| {
                    writer.write_all(&[b'\n'])?;
                    serde_json::to_writer(&mut writer, log)?;
                    writer.write_all(&[b'\n'])?;
                    Ok(())
                })?;
        }

        Ok((written_bytes, byte_size))
    }
}

fn write_bulk_action(
    writer: &mut dyn Write,
    bulk_action: &str,
    index: &str,
    doc_type: &str,
    suppress_type: bool,
    document: &DocumentMetadata,
) -> std::io::Result<usize> {
    as_tracked_write(
        writer,
        (bulk_action, index, doc_type, suppress_type, document),
        |writer, (bulk_action, index, doc_type, suppress_type, document)| match (
            suppress_type,
            document,
        ) {
            (true, DocumentMetadata::Id(id)) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_id":"{}"}}}}"#,
                    bulk_action, index, id
                )
            }
            (false, DocumentMetadata::Id(id)) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_type":"{}","_id":"{}"}}}}"#,
                    bulk_action, index, doc_type, id
                )
            }
            (true, DocumentMetadata::WithoutId) => {
                write!(writer, r#"{{"{}":{{"_index":"{}"}}}}"#, bulk_action, index)
            }
            (false, DocumentMetadata::WithoutId) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_type":"{}"}}}}"#,
                    bulk_action, index, doc_type
                )
            }
            (true, DocumentMetadata::IdAndVersion(id, version)) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_id":"{}","version_type":"{}","version":{}}}}}"#,
                    bulk_action,
                    index,
                    id,
                    version.kind.as_str(),
                    version.value
                )
            }
            (false, DocumentMetadata::IdAndVersion(id, version)) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_type":"{}","_id":"{}","version_type":"{}","version":{}}}}}"#,
                    bulk_action,
                    index,
                    doc_type,
                    id,
                    version.kind.as_str(),
                    version.value
                )
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppress_type_with_id() {
        let mut writer = Vec::new();

        _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            true,
            &DocumentMetadata::Id("ID".to_string()),
        );

        let value: serde_json::Value = serde_json::from_slice(&writer).unwrap();
        let value = value.as_object().unwrap();

        assert!(value.contains_key("ACTION"));

        let nested = value.get("ACTION").unwrap();
        let nested = nested.as_object().unwrap();

        assert!(nested.contains_key("_index"));
        assert_eq!(nested.get("_index").unwrap().as_str(), Some("INDEX"));
        assert!(nested.contains_key("_id"));
        assert_eq!(nested.get("_id").unwrap().as_str(), Some("ID"));
        assert!(!nested.contains_key("_type"));
    }

    #[test]
    fn suppress_type_without_id() {
        let mut writer = Vec::new();

        _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            true,
            &DocumentMetadata::WithoutId,
        );

        let value: serde_json::Value = serde_json::from_slice(&writer).unwrap();
        let value = value.as_object().unwrap();

        assert!(value.contains_key("ACTION"));

        let nested = value.get("ACTION").unwrap();
        let nested = nested.as_object().unwrap();

        assert!(nested.contains_key("_index"));
        assert_eq!(nested.get("_index").unwrap().as_str(), Some("INDEX"));
        assert!(!nested.contains_key("_id"));
        assert!(!nested.contains_key("_type"));
    }

    #[test]
    fn type_with_id() {
        let mut writer = Vec::new();

        _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            false,
            &DocumentMetadata::Id("ID".to_string()),
        );

        let value: serde_json::Value = serde_json::from_slice(&writer).unwrap();
        let value = value.as_object().unwrap();

        assert!(value.contains_key("ACTION"));

        let nested = value.get("ACTION").unwrap();
        let nested = nested.as_object().unwrap();

        assert!(nested.contains_key("_index"));
        assert_eq!(nested.get("_index").unwrap().as_str(), Some("INDEX"));
        assert!(nested.contains_key("_id"));
        assert_eq!(nested.get("_id").unwrap().as_str(), Some("ID"));
        assert!(nested.contains_key("_type"));
        assert_eq!(nested.get("_type").unwrap().as_str(), Some("TYPE"));
    }

    #[test]
    fn type_without_id() {
        let mut writer = Vec::new();

        _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            false,
            &DocumentMetadata::WithoutId,
        );

        let value: serde_json::Value = serde_json::from_slice(&writer).unwrap();
        let value = value.as_object().unwrap();

        assert!(value.contains_key("ACTION"));

        let nested = value.get("ACTION").unwrap();
        let nested = nested.as_object().unwrap();

        assert!(nested.contains_key("_index"));
        assert_eq!(nested.get("_index").unwrap().as_str(), Some("INDEX"));
        assert!(!nested.contains_key("_id"));
        assert!(nested.contains_key("_type"));
        assert_eq!(nested.get("_type").unwrap().as_str(), Some("TYPE"));
    }
}
