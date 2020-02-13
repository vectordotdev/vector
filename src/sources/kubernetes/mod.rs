#[cfg(test)]
mod test;

use crate::{
    event::{self, Event, Value},
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
use chrono::{DateTime, Utc};
use futures::{sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
// ?NOTE
// Original proposal: https://github.com/kubernetes/kubernetes/blob/release-1.5/docs/proposals/kubelet-cri-logging.md#proposed-solution
// Intermediate version: https://github.com/kubernetes/community/blob/master/contributors/design-proposals/node/kubelet-cri-logging.md#proposed-solution
// Current version: https://github.com/kubernetes/kubernetes/tree/master/staging/src/k8s.io/cri-api/pkg/apis/runtime/v1alpha2
// LogDirectory = `/var/log/pods/<podUID>/`
// LogPath = `containerName/Instance#.log`

/// Location in which by Kubernetes CRI, container runtimes are to store logs.
const LOG_DIRECTORY: &str = r"/var/log/pods/";

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
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
        let mut transform_pod_uid = transform_pod_uid()?;
        let mut parse_message = parse_message()?;

        // Kubernetes source
        let source = file_recv
            .filter_map(move |event| transform_file.transform(event))
            .filter_map(move |event| parse_message.transform(event))
            .filter_map(move |event| now.filter(event))
            .map(remove_ending_newline)
            .filter_map(move |event| transform_pod_uid.transform(event))
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
        if let Some(Value::Timestamp(ts)) = event.as_log().get(&event::log_schema().timestamp_key())
        {
            if ts < &self.start {
                trace!(message = "Recieved older log.", from = %ts.to_rfc3339());
                return None;
            }
        }
        Some(event)
    }
}

fn file_source(
    kube_name: &str,
    globals: &GlobalOptions,
) -> crate::Result<(mpsc::Receiver<Event>, Source)> {
    let mut config = FileConfig::default();

    // TODO: Add exclude_namspace option, and with it, in config, exclude kube-system namespace.
    // This is correct, but on best effort basis filtering out of logs from kuberentes system components.
    // More specificly, it will work for all Kubernetes 1.14 and higher, and for some bellow that.
    config
        .exclude
        .push((LOG_DIRECTORY.to_owned() + r"kube-system_*").into());

    // TODO: Add exclude_namspace option, and with it, in config, exclude namespace used by vector.
    // NOTE: for now exclude images with name vector, it's a rough solution, but necessary for now
    config
        .exclude
        .push((LOG_DIRECTORY.to_owned() + r"*/vector*").into());

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

/// Determines format of message.
/// This exists because Docker is still a special entity in Kubernetes as it can write in Json
/// despite CRI defining it's own format.
fn parse_message() -> crate::Result<ApplicableTransform> {
    let transforms = vec![
        Box::new(transform_json_message()) as Box<dyn Transform>,
        transform_cri_message()?,
    ];
    Ok(ApplicableTransform::Candidates(transforms))
}

fn remove_ending_newline(mut event: Event) -> Event {
    if let Some(Value::Bytes(msg)) = event
        .as_mut_log()
        .get_mut(&event::log_schema().timestamp_key())
    {
        if msg.ends_with(&['\n' as u8]) {
            msg.truncate(msg.len() - 1);
        }
    }
    event
}

#[derive(Debug)]
struct DockerMessageTransformer {
    json_parser: JsonParser,
    atom_time: Atom,
    atom_log: Atom,
}

impl Transform for DockerMessageTransformer {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut event = self.json_parser.transform(event)?;

        // Rename fields
        let log = event.as_mut_log();

        // time -> timestamp
        if let Some(Value::Bytes(timestamp_bytes)) = log.remove(&self.atom_time) {
            match DateTime::parse_from_rfc3339(
                String::from_utf8_lossy(timestamp_bytes.as_ref()).as_ref(),
            ) {
                Ok(timestamp) => log.insert(
                    event::log_schema().timestamp_key().clone(),
                    timestamp.with_timezone(&Utc),
                ),
                Err(error) => {
                    debug!(message = "Non rfc3339 timestamp.", %error, rate_limit_secs = 10);
                    return None;
                }
            }
        } else {
            debug!(message = "Missing field.", field = %self.atom_time, rate_limit_secs = 10);
            return None;
        }

        // log -> message
        if let Some(message) = log.remove(&self.atom_log) {
            log.insert(event::log_schema().message_key().clone(), message);
        } else {
            debug!(message = "Missing field.", field = %self.atom_log, rate_limit_secs = 10);
            return None;
        }

        Some(event)
    }
}

/// As defined by Docker
fn transform_json_message() -> DockerMessageTransformer {
    let mut config = JsonParserConfig::default();

    // Drop so that it's possible to detect if message is in json format
    config.drop_invalid = true;

    config.drop_field = true;

    DockerMessageTransformer {
        json_parser: config.into(),
        atom_time: Atom::from("time"),
        atom_log: Atom::from("log"),
    }
}

/// As defined by CRI
fn transform_cri_message() -> crate::Result<Box<dyn Transform>> {
    let mut rp_config = RegexParserConfig::default();
    // message field
    rp_config.regex =
        r"^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)$"
            .to_owned();
    // drop field
    rp_config.types.insert(
        event::log_schema().timestamp_key().clone(),
        "timestamp|%+".to_owned(),
    );
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

/// Contains several regexes that can parse common forms of pod_uid.
/// On the first message, regexes are tried out one after the other until
/// first succesfull one has been found. After that that regex will be
/// always used.
///
/// If nothing succeeds the message is still passed.
fn transform_pod_uid() -> crate::Result<ApplicableTransform> {
    let mut regexes = Vec::new();

    let namespace_regex = r"(?P<pod_namespace>[0-9a-z.\-]*)";
    let name_regex = r"(?P<pod_name>[0-9a-z.\-]*)";
    let uid_regex = r"(?P<object_uid>([0-9A-Fa-f]{8}[-][0-9A-Fa-f]{4}[-][0-9A-Fa-f]{4}[-][0-9A-Fa-f]{4}[-][0-9A-Fa-f]{12}|[0-9A-Fa-f]{32}))";

    // Definition of pod_uid has been well defined since Kubernetes 1.14 with https://github.com/kubernetes/kubernetes/pull/74441

    // Minikube 1.15, MicroK8s 1.15,1.14,1.16 , DigitalOcean 1.16 , Google Kubernetes Engine 1.13, 1.14, EKS 1.14
    // format: namespace_name_UID
    regexes.push(format!(
        "^{}_{}_{}$",
        namespace_regex, name_regex, uid_regex
    ));

    // EKS 1.13 , AKS 1.13.12, MicroK8s 1.13
    // If everything else fails, try to at least parse out uid from somewhere.
    // This is somewhat robust as UUID format is hard to create by accident
    // ,at least in this context, plus regex requires that UUID is separated
    // from other data either by start,end of string or by non UUID character.
    regexes.push(format!(
        r"(^|[^0-9A-Fa-f\-]){}([^0-9A-Fa-f\-]|$)",
        uid_regex
    ));

    let mut transforms = Vec::new();
    for regex in regexes {
        let mut config = RegexParserConfig::default();

        config.field = Some("pod_uid".into());
        config.regex = regex;
        // Remove pod_uid as it isn't usable anywhere else.
        config.drop_field = true;
        config.drop_failed = true;

        let transform = RegexParser::build(&config).map_err(|e| {
            format!(
                "Failed in creating pod_uid regex transform with error: {:?}",
                e
            )
        })?;
        transforms.push(transform);
    }

    Ok(ApplicableTransform::Candidates(transforms))
}

/// Contains several transforms. On the first message, transforms are tried
/// out one after the other until the first successful one has been found.
/// After that the transform will always be used.
///
/// If nothing succeds the message is still passed.
enum ApplicableTransform {
    Candidates(Vec<Box<dyn Transform>>),
    Transform(Option<Box<dyn Transform>>),
}

impl Transform for ApplicableTransform {
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self {
            Self::Candidates(candidates) => {
                let candidate = candidates
                    .iter_mut()
                    .enumerate()
                    .find_map(|(i, t)| t.transform(event.clone()).map(|event| (i, event)));
                if let Some((i, event)) = candidate {
                    let candidate = candidates.remove(i);
                    *self = Self::Transform(Some(candidate));
                    Some(event)
                } else {
                    *self = Self::Transform(None);
                    warn!("No applicable transform.");
                    None
                }
            }
            Self::Transform(Some(transform)) => transform.transform(event),
            Self::Transform(None) => Some(event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has<V: Into<Value>>(event: &Event, field: &str, data: V) {
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
        event.as_mut_log().insert("file","/var/log/pods/default_busybox-echo-5bdc7bfd99-m996l_e2782fb0-ba64-4289-acd5-68c4f5b0d27e/busybox/3.log".to_owned());

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
        event.as_mut_log().insert(
            "message",
            "2019-10-02T13:21:36.927620189+02:00 stdout F 12".to_owned(),
        );

        let mut transform = transform_cri_message().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, event::log_schema().message_key().as_ref(), "12");
        has(&event, "multiline_tag", "F");
        has(&event, "stream", "stdout");
        has(
            &event,
            event::log_schema().timestamp_key().as_ref(),
            DateTime::parse_from_rfc3339("2019-10-02T13:21:36.927620189+02:00")
                .unwrap()
                .with_timezone(&Utc),
        );
    }

    #[test]
    fn pod_uid_transform_namespace_name_uid() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert(
            "pod_uid",
            "kube-system_kube-apiserver-minikube_8f6b5d95bfe4bcf4cc9c4d8435f0668b".to_owned(),
        );

        let mut transform = transform_pod_uid().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, "pod_namespace", "kube-system");
        has(&event, "pod_name", "kube-apiserver-minikube");
        has(&event, "object_uid", "8f6b5d95bfe4bcf4cc9c4d8435f0668b");
    }

    #[test]
    fn pod_uid_transform_uid() {
        let mut event = Event::new_empty_log();
        event
            .as_mut_log()
            .insert("pod_uid", "306cd636-0c6d-11ea-9079-1c1b0de4d755".to_owned());

        let mut transform = transform_pod_uid().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, "object_uid", "306cd636-0c6d-11ea-9079-1c1b0de4d755");
    }
}
