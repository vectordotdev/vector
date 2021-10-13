use crate::sinks::util::encoding::{Encoder, as_tracked_write};
use crate::event::{LogEvent, Finalizable, EventFinalizers};
use std::io::Write;

use crate::sinks::elasticsearch::{ElasticSearchCommonMode, BulkAction};
use serde_json::json;

use crate::internal_events::ElasticSearchEventEncoded;


use std::io;
use vector_core::ByteSizeOf;

pub struct ProcessedEvent {
    pub index: String,
    pub bulk_action: BulkAction,
    pub log: LogEvent,
    pub id: Option<String>
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

pub struct ElasticSearchEncoder {
    pub mode: ElasticSearchCommonMode,
    pub doc_type: String,
}

impl Encoder<Vec<ProcessedEvent>> for ElasticSearchEncoder {
    fn encode_input(&self, input: Vec<ProcessedEvent>, writer: &mut dyn Write) -> std::io::Result<usize> {
        let mut written_bytes = 0;
        for event in input {

            // TODO: (perf): use a struct here instead of json Value
            let mut action = json!({
                event.bulk_action.as_str(): {
                    "_index": event.index,
                    "_type": self.doc_type,
                }
            });

            if let Some(id) = event.id {
                // TODO: please get rid of this
                let doc = action.pointer_mut(event.bulk_action.as_json_pointer()).unwrap();
                doc.as_object_mut()
                    .unwrap()
                    .insert("_id".into(), json!(id));
            }

            written_bytes += as_tracked_write::<_,_,io::Error>(writer, (&action, &event.log), |mut writer, (action, log)| {
                serde_json::to_writer(&mut writer, action)?;
                writer.write_all(&[b'\n'])?;
                serde_json::to_writer(&mut writer, log)?;
                writer.write_all(&[b'\n'])?;
                Ok(())
            })?;

            //TODO: split into trace log + batched written bytes?
            emit!(&ElasticSearchEventEncoded {
                byte_size: written_bytes,
                index: event.index,
            });
        }
        Ok(written_bytes)
    }
}
