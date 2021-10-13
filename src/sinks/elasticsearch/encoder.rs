use crate::sinks::util::encoding::{Encoder, LogEncoder, as_tracked_write};
use crate::event::{Event, LogEvent};
use std::io::Write;
use crate::transforms::metric_to_log::MetricToLog;
use crate::sinks::elasticsearch::{ElasticSearchCommonMode, maybe_set_id, BulkAction};
use serde_json::json;
use serde::{Serialize, Deserialize};
use crate::internal_events::ElasticSearchEventEncoded;
use vector_core::event::EventRef;
use crate::sinks::elasticsearch::sink::BatchedEvents;
use std::io;

pub struct ProcessedEvent {
    pub index: String,
    pub bulk_action: BulkAction,
    pub log: LogEvent,
    pub id: Option<String>
}

pub struct ElasticSearchEncoder {
    mode: ElasticSearchCommonMode,
    doc_type: String,
}

impl Encoder<Vec<ProcessedEvent>> for ElasticSearchEncoder {
    fn encode_input(&self, mut input: Vec<ProcessedEvent>, writer: &mut dyn Write) -> std::io::Result<usize> {
        let mut written_bytes = 0;
        for mut event in input {

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
