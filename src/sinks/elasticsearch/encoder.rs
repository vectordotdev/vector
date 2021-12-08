use crate::event::{EventFinalizers, Finalizable, LogEvent};
use crate::sinks::util::encoding::{as_tracked_write, Encoder, VisitLogMut};
use std::io::Write;

use crate::sinks::elasticsearch::BulkAction;

use crate::internal_events::ElasticSearchEventEncoded;

use std::io;
use vector_core::ByteSizeOf;

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
        |writer, (bulk_action, index, doc_type, id, suppress_type)| {
            if let Some(id) = id {
                if suppress_type {
                    write!(
                        writer,
                        r#"{{"{}":{{"_index":"{}","_id":"{}"}}}}"#,
                        bulk_action, index, id
                    )
                } else {
                    write!(
                        writer,
                        r#"{{"{}":{{"_index":"{}","_type":"{}","_id":"{}"}}}}"#,
                        bulk_action, index, doc_type, id
                    )
                }
            } else if suppress_type {
                    write!(writer, r#"{{"{}":{{"_index":"{}"}}}}"#, bulk_action, index)
                } else {
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
