use crate::{
    event::{self, Event, ValueKind},
    sources::file::{FileConfig, FingerprintingConfig},
    topology::config::{DataType, GlobalOptions, SourceConfig, TransformConfig},
    transforms::{regex_parser::RegexParserConfig, remove_fields::RemoveFields, Transform},
};
use bytes::Bytes;
use chrono;
use futures::{sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

/// Location in which by Kubernetes CRI, container runtimes are to store logs.
const LOG_DIRECTORY: &'static str = r"/var/log/pods/";

// ?NOTE: Maybe having pod uid exposed as config field is a better approach.
/// Kubernetes source expects it's pod uid in this env var.
/// If it's not present it assumes that it's outside Kubernetes managment.
const ENV_VAR_POD_UID: &'static str = "POD_UID";

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KubernetesConfig {}

#[typetag::serde(name = "kubernetes")]
impl SourceConfig for KubernetesConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        // Kubernetes source uses 'file source' and 'regex transform' to implement
        // gathering of logs over all Kubernetes CRI supported container runtimes.

        // Side goal is to make kubernetes source behave as simillarly to docker source
        // as possible to set a default behavior for all container related sources.
        // This will help with interchangeability.

        // Only logs created at, or after this moment are logged.
        let now = chrono::Utc::now();

        // Tailling logs are limited to those that can have logs created during or
        // after now.

        // File source
        let file_source_name = name.to_owned() + "_file_source";
        let (file_recv, file_source) = file_source(file_source_name.as_str(), globals)?;

        // Remove field, timestamp
        // This field is removed as we will supply our own timestamp gotten directly
        // from container runtime.
        let mut remove_timestamp = RemoveFields::new(vec![event::TIMESTAMP.clone()]);

        // Regex transform, message
        let mut regex_transform_message = transform_message()?;

        // Regex transform, file
        let mut regex_transform_file = transform_file()?;

        // Is this in Kubernetes
        let self_pod = std::env::var(ENV_VAR_POD_UID).ok().map(|uid| {
            info!(message = "Self pod", uid = %uid);
            Bytes::from(uid)
        });

        // Kubernetes source
        let pod = Atom::from("pod");
        let source = file_recv
            .filter_map(move |event| remove_timestamp.transform(event))
            .filter_map(move |event| regex_transform_message.transform(event))
            .filter_map(move |event| {
                // Only logs created at, or after now are logged.
                if let Some(ValueKind::Timestamp(ts)) = event.as_log().get(&event::TIMESTAMP) {
                    if ts >= &now {
                        return Some(event);
                    }
                }
                None
            })
            .filter_map(move |event| regex_transform_file.transform(event))
            .filter_map(move |event| {
                // Detect self and exclude own messages
                if let Some(self_pod) = self_pod.as_ref() {
                    if let Some(ValueKind::Bytes(pod)) = event.as_log().get(&pod) {
                        if pod == self_pod {
                            return None;
                        }
                    }
                }
                Some(event)
            })
            .forward(out.sink_map_err(|_| ()))
            .map(|_| ())
            .join(file_source)
            .map(|((), ())| ());

        Ok(Box::new(source))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

fn file_source(
    name: &str,
    globals: &GlobalOptions,
) -> Result<(mpsc::Receiver<Event>, super::Source), String> {
    let mut fs_config = FileConfig::default();
    fs_config
        .include
        .push((LOG_DIRECTORY.to_owned() + r"*/[!_]*_[!.]*\.log").into());
    fs_config.start_at_beginning = true;
    // Filter out files that certainly don't have logs newer than now timestamp.
    fs_config.ignore_older = Some(10);
    // oldest_first false, having all pods equaly serviced is of greater importance
    //                     than having time order guarantee.
    // Timestamps contained in the file will practically make it impossible to mistake
    // two files as one
    fs_config.fingerprinting = FingerprintingConfig::Checksum {
        fingerprint_bytes: 1024, // The goal is to cover at least two timestamps
        ignored_header_bytes: 0,
    };

    let (file_send, file_recv) = mpsc::channel(1000);
    let file_source = fs_config
        .build(name, globals, file_send)
        .map_err(|e| format!("Failed in creating file source with error: {:?}", e))?;

    Ok((file_recv, file_source))
}

fn transform_message() -> Result<Box<dyn Transform>, String> {
    let mut rp_config = RegexParserConfig::default();
    // message field
    rp_config.regex = r"^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<message>.*)$".to_owned();
    // drop field
    rp_config
        .types
        .insert(event::TIMESTAMP.clone(), "timestamp|%+".to_owned());
    // stream is a string
    // message is a string
    rp_config.build().map_err(|e| {
        format!(
            "Failed in creating message regex transform with error: {:?}",
            e
        )
    })
}

fn transform_file() -> Result<Box<dyn Transform>, String> {
    let mut rp_config = RegexParserConfig::default();
    rp_config.field = Some("file".into());
    rp_config.regex = r"^".to_owned()
        + LOG_DIRECTORY
        + r"(?P<pod>[^/]*)/(?P<container_name>[^_]*)_(?P<container>[^.]*)\.log$";
    // drop field, this field is implementation depended so remove it
    // pod is a string
    // container_name is a string
    // container is a string
    rp_config.build().map_err(|e| {
        format!(
            "Failed in creating file regex transform with error: {:?}",
            e
        )
    })
}
