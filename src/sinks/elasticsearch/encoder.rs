use std::{io, io::Write};

use vector_core::ByteSizeOf;

use crate::{
    event::{EventFinalizers, Finalizable, LogEvent},
    internal_events::ElasticSearchEventEncoded,
    sinks::{
        elasticsearch::BulkAction,
        util::encoding::{as_tracked_write, Encoder, VisitLogMut},
    },
};

pub struct ProcessedEvent {
    pub index: String,
    pub bulk_action: BulkAction,
    pub log: LogEvent,
    pub id: Option<String>,
}

impl Finalizable for ProcessedEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.log.metadata_mut().take_finalizers()
    }
}

impl ByteSizeOf for ProcessedEvent {
    fn allocated_bytes(&self) -> usize {
        self.index.allocated_bytes() + self.log.allocated_bytes() + self.id.allocated_bytes()
    }
}

#[derive(PartialEq, Default, Clone, Debug)]
pub struct ElasticSearchEncoder {
    pub doc_type: String,
    pub suppress_type_name: bool,
}

impl Encoder<Vec<ProcessedEvent>> for ElasticSearchEncoder {
    fn encode_input(
        &self,
        input: Vec<ProcessedEvent>,
        writer: &mut dyn Write,
    ) -> std::io::Result<usize> {
        let mut written_bytes = 0;
        for event in input {
            written_bytes += write_bulk_action(
                writer,
                event.bulk_action.as_str(),
                &event.index,
                &self.doc_type,
                self.suppress_type_name,
                &event.id,
            )?;
            written_bytes +=
                as_tracked_write::<_, _, io::Error>(writer, &event.log, |mut writer, log| {
                    writer.write_all(&[b'\n'])?;
                    serde_json::to_writer(&mut writer, log)?;
                    writer.write_all(&[b'\n'])?;
                    Ok(())
                })?;

            emit!(&ElasticSearchEventEncoded {
                byte_size: written_bytes,
                index: event.index,
            });
        }
        Ok(written_bytes)
    }
}

fn write_bulk_action(
    writer: &mut dyn Write,
    bulk_action: &str,
    index: &str,
    doc_type: &str,
    suppress_type: bool,
    id: &Option<String>,
) -> std::io::Result<usize> {
    as_tracked_write(
        writer,
        (bulk_action, index, doc_type, id, suppress_type),
        |writer, (bulk_action, index, doc_type, id, suppress_type)| match (id, suppress_type) {
            (Some(id), true) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_id":"{}"}}}}"#,
                    bulk_action, index, id
                )
            }
            (Some(id), false) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_type":"{}","_id":"{}"}}}}"#,
                    bulk_action, index, doc_type, id
                )
            }
            (None, true) => {
                write!(writer, r#"{{"{}":{{"_index":"{}"}}}}"#, bulk_action, index)
            }
            (None, false) => {
                write!(
                    writer,
                    r#"{{"{}":{{"_index":"{}","_type":"{}"}}}}"#,
                    bulk_action, index, doc_type
                )
            }
        },
    )
}

impl VisitLogMut for ProcessedEvent {
    fn visit_logs_mut<F>(&mut self, func: F)
    where
        F: Fn(&mut LogEvent),
    {
        func(&mut self.log);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppress_type_with_id() {
        let mut writer = Vec::new();

        let _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            true,
            &Some("ID".to_string()),
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

        let _ = write_bulk_action(&mut writer, "ACTION", "INDEX", "TYPE", true, &None);

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

        let _ = write_bulk_action(
            &mut writer,
            "ACTION",
            "INDEX",
            "TYPE",
            false,
            &Some("ID".to_string()),
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

        let _ = write_bulk_action(&mut writer, "ACTION", "INDEX", "TYPE", false, &None);

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
