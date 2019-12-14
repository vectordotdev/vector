#[cfg(test)]
mod test;

use crate::{
    event::{self, Event, ValueKind},
    sources::{
        file::{FileConfig, FingerprintingConfig},
        Source,
    },
    topology::config::{DataType, GlobalOptions, SourceConfig},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        regex_parser::{RegexParser, RegexParserConfig},
        Transform,
    },
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;
// ?NOTE
// Original proposal: https://github.com/kubernetes/kubernetes/blob/release-1.5/docs/proposals/kubelet-cri-logging.md#proposed-solution
// Intermediate version: https://github.com/kubernetes/community/blob/master/contributors/design-proposals/node/kubelet-cri-logging.md#proposed-solution
// Current version: https://github.com/kubernetes/kubernetes/tree/master/staging/src/k8s.io/cri-api/pkg/apis/runtime/v1alpha2
// LogDirectory = `/var/log/pods/<podUID>/`
// LogPath = `containerName/Instance#.log`

/// Location in which by Kubernetes CRI, container runtimes are to store logs.
const LOG_DIRECTORY: &str = r"/var/log/pods/";

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
    ) -> crate::Result<Source> {
        // Kubernetes source uses 'file source' and various transforms to implement
        // gathering of logs over Kubernetes CRI supported container runtimes.

        // Side goal is to make kubernetes source behave as simillarly to docker source
        // as possible to set a default behavior for all container related sources.
        // This will help with interchangeability.

        let now = TimeFilter::new();

        let (file_recv, file_source) = file_source(name, globals)?;

        let mut transform_file = transform_file()?;
        let mut parse_message = parse_message()?;

        // Kubernetes source
        let source = file_recv
            .filter_map(move |event| transform_file.transform(event))
            .filter_map(move |event| parse_message(event))
            .filter_map(move |event| now.filter(event))
            .forward(out.sink_map_err(drop))
            .map(drop)
            .join(file_source)
            .map(drop);

        Ok(Box::new(source))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "kubernetes"
    }
}

/// In what format are logs
#[derive(Clone, Copy, Debug)]
enum LogFormat {
    /// As defined by Docker
    Json,
    /// As defined by CRI
    CRI,
    /// None of the above
    Unknown,
}

/// Data known about Kubernetes Pod
#[derive(Default)]
struct PodData {
    /// This exists because Docker is still a special entity in Kubernetes as it can write in Json
    /// despite CRI defining it's own format.
    log_format: Option<LogFormat>,
}

struct TimeFilter {
    start: DateTime<Utc>,
}

impl TimeFilter {
    fn new() -> Self {
        // Only logs created at, or after this moment are logged.
        let now = Utc::now();
        TimeFilter { start: now }
    }

    fn filter(&self, event: Event) -> Option<Event> {
        // Only logs created at, or after now are logged.
        if let Some(ValueKind::Timestamp(ts)) = event.as_log().get(&event::TIMESTAMP) {
            if ts >= &self.start {
                return Some(event);
            }
            trace!(message = "Recieved older log.", from = %ts.to_rfc3339());
        }
        None
    }
}

fn file_source(
    kube_name: &str,
    globals: &GlobalOptions,
) -> crate::Result<(mpsc::Receiver<Event>, Source)> {
    let mut config = FileConfig::default();

    // TODO: Having a configurable option for excluding namespaces, seams to be usefull.
    // // TODO: Find out if there are some guarantee from Kubernetes that current build of
    // // TODO  pod_uid as namespace_pod-name_some-number is a somewhat lasting decision.
    // NOTE: pod_uid is unspecified and it has been found that on EKS it has different scheme.
    // NOTE: as such excluding/including using path is hacky, instead, more proper source of
    // NOTE: information should be used.
    // NOTE: At best, excluding/including using path can be an optimization
    // TODO: Exclude whole kube-system namespace properly
    // TODO: Add exclude_namspace option, and with it in config exclude namespace used by vector.
    // NOTE: for now exclude images with name vector, it's a rough solution, but necessary for now
    config
        .exclude
        .push((LOG_DIRECTORY.to_owned() + r"*/vector*/*").into());

    config
        .include
        .push((LOG_DIRECTORY.to_owned() + r"*/*/*.log").into());

    config.start_at_beginning = true;

    // Filter out files that certainly don't have logs newer than now timestamp.
    config.ignore_older = Some(10);

    // oldest_first false, having all pods equaly serviced is of greater importance
    //                     than having time order guarantee.

    // CRI standard ensures unique naming.
    config.fingerprinting = FingerprintingConfig::DevInode;

    // Have a subdirectory for this source to avoid collision of naming its file source.
    config.data_dir = Some(globals.resolve_and_make_data_subdir(None, kube_name)?);

    let (file_send, file_recv) = mpsc::channel(1000);
    let file_source = config
        .build("file_source", globals, file_send)
        .map_err(|e| format!("Failed in creating file source with error: {:?}", e))?;

    Ok((file_recv, file_source))
}

fn parse_message() -> crate::Result<impl FnMut(Event) -> Option<Event> + Send> {
    // Transforms
    let mut transform_json_message = transform_json_message();
    let mut transform_cri_message = transform_cri_message()?;

    // Kubernetes source
    let atom_pod_uid = Atom::from("pod_uid");
    let mut pod_data = HashMap::<Bytes, PodData>::new();
    Ok(move |event: Event| {
        let log = event.as_log();
        if let Some(ValueKind::Bytes(pod_uid)) = log.get(&atom_pod_uid) {
            // Fetch pod data
            let data = pod_data.entry(pod_uid.clone()).or_default();

            // Get/determine log format
            let log_format = if let Some(log_format) = data.log_format {
                log_format
            } else {
                // Detect log format
                let log_format = if transform_json_message(event.clone()).is_some() {
                    LogFormat::Json
                } else if transform_cri_message.transform(event.clone()).is_some() {
                    LogFormat::CRI
                } else {
                    error!(message = "Unknown message format");
                    // Return untouched message so that user has some options
                    // in how to deal with it.
                    LogFormat::Unknown
                };
                // Save log format
                data.log_format = Some(log_format);
                log_format
            };

            // Parse message
            match log_format {
                LogFormat::Json => transform_json_message(event),
                LogFormat::CRI => transform_cri_message.transform(event),
                LogFormat::Unknown => Some(event),
            }
        } else {
            None
        }
    })
}

fn transform_json_message() -> impl FnMut(Event) -> Option<Event> + Send {
    let mut config = JsonParserConfig::default();

    // Drop so that it's possible to detect if message is in json format
    config.drop_invalid = true;

    config.drop_field = true;

    let mut json_parser: JsonParser = config.into();

    let atom_time = Atom::from("time");
    let atom_log = Atom::from("log");
    move |event| {
        let mut event = json_parser.transform(event)?;

        // Rename fields
        let log = event.as_mut_log();

        // time -> timestamp
        if let Some(ValueKind::Bytes(timestamp_bytes)) = log.remove(&atom_time) {
            match DateTime::parse_from_rfc3339(
                String::from_utf8_lossy(timestamp_bytes.as_ref()).as_ref(),
            ) {
                Ok(timestamp) => log.insert_explicit(
                    event::TIMESTAMP.clone(),
                    timestamp.with_timezone(&Utc).into(),
                ),
                Err(error) => warn!(message = "Non rfc3339 timestamp.", %error),
            }
        } else {
            warn!(message = "Missing field.", field = %atom_time);
        }

        // log -> message
        if let Some(message) = log.remove(&atom_log) {
            log.insert_explicit(event::MESSAGE.clone(), message);
        } else {
            warn!(message = "Missing field", field = %atom_log);
        }

        Some(event)
    }
}

fn transform_cri_message() -> crate::Result<Box<dyn Transform>> {
    let mut rp_config = RegexParserConfig::default();
    // message field
    rp_config.regex =
        r"^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)$"
            .to_owned();
    // drop field
    rp_config
        .types
        .insert(event::TIMESTAMP.clone(), "timestamp|%+".to_owned());
    // stream is a string
    // message is a string
    RegexParser::build(&rp_config).map_err(|e| {
        format!(
            "Failed in creating message regex transform with error: {:?}",
            e
        )
        .into()
    })
}

fn transform_file() -> crate::Result<Box<dyn Transform>> {
    let mut config = RegexParserConfig::default();

    config.field = Some("file".into());

    config.regex = r"^".to_owned()
        + LOG_DIRECTORY
        + r"(?P<pod_uid>[^/]*)/(?P<container_name>[^/]*)/[0-9]*[.]log$";

    // this field is implementation depended so remove it
    config.drop_field = true;

    // pod_uid is a string
    // container_name is a string
    RegexParser::build(&config).map_err(|e| {
        format!(
            "Failed in creating file regex transform with error: {:?}",
            e
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has<V: Into<ValueKind>>(event: &Event, field: &str, data: V) {
        assert_eq!(
            event
                .as_log()
                .get(&field.into())
                .expect(format!("field: {:?} not present", field).as_str()),
            &data.into()
        );
    }

    #[test]
    fn file_path_transform() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert_explicit("file".into(),"/var/log/pods/default_busybox-echo-5bdc7bfd99-m996l_e2782fb0-ba64-4289-acd5-68c4f5b0d27e/busybox/3.log".to_owned().into());

        let mut transform = transform_file().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, "container_name", "busybox");
        has(
            &event,
            "pod_uid",
            "default_busybox-echo-5bdc7bfd99-m996l_e2782fb0-ba64-4289-acd5-68c4f5b0d27e",
        );
    }

    #[test]
    fn cri_message_transform() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert_explicit(
            "message".into(),
            "2019-10-02T13:21:36.927620189+02:00 stdout F 12"
                .to_owned()
                .into(),
        );

        let mut transform = transform_cri_message().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, event::MESSAGE.as_ref(), "12");
        has(&event, "multiline_tag", "F");
        has(&event, "stream", "stdout");
        has(
            &event,
            event::TIMESTAMP.as_ref(),
            DateTime::parse_from_rfc3339("2019-10-02T13:21:36.927620189+02:00")
                .unwrap()
                .with_timezone(&Utc),
        );
    }
}
