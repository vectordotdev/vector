#[cfg(test)]
pub mod test;

mod applicable_transform;
mod file_source_builder;
mod message_parser;

use self::applicable_transform::ApplicableTransform;
use crate::{
    event::{self, Event, Value},
    shutdown::ShutdownSignal,
    sources::Source,
    topology::config::{DataType, GlobalOptions, SourceConfig},
    transforms::{
        regex_parser::{RegexParser, RegexParserConfig},
        Transform,
    },
};
use chrono::{DateTime, Utc};
use futures01::{sync::mpsc, Future, Sink, Stream};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use string_cache::DefaultAtom as Atom;

// ?NOTE
// Original proposal: https://github.com/kubernetes/kubernetes/blob/release-1.5/docs/proposals/kubelet-cri-logging.md#proposed-solution
// Intermediate version: https://github.com/kubernetes/community/blob/master/contributors/design-proposals/node/kubelet-cri-logging.md#proposed-solution
// Current version: https://github.com/kubernetes/kubernetes/tree/master/staging/src/k8s.io/cri-api/pkg/apis/runtime/v1alpha2
// LogDirectory = `/var/log/pods/<podUID>/`
// LogPath = `containerName/Instance#.log`

/// Location in which by Kubernetes CRI, container runtimes are to store logs.
const LOG_DIRECTORY: &str = r"/var/log/pods/";

lazy_static! {
    pub static ref POD_UID: Atom = Atom::from("object_uid");
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("To large UID: {:?}", uid))]
    UidToLarge { uid: String },
    #[snafu(display("UID contains illegal characters: {:?}", uid))]
    IllegalCharacterInUid { uid: String },
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct KubernetesConfig {
    include_container_names: Vec<String>,
    include_pod_uids: Vec<String>,
    include_namespaces: Vec<String>,
}

#[typetag::serde(name = "kubernetes")]
impl SourceConfig for KubernetesConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<Source> {
        // Kubernetes source uses 'file source' and various transforms to implement
        // gathering of logs over Kubernetes CRI supported container runtimes.

        // Side goal is to make kubernetes source behave as simillarly to docker source
        // as possible to set a default behavior for all container related sources.
        // This will help with interchangeability.

        let now = TimeFilter::new();

        let (file_recv, file_source) =
            file_source_builder::FileSourceBuilder::new(self).build(name, globals, shutdown)?;

        let mut transform_file = transform_file()?;
        let mut transform_pod_uid = transform_pod_uid()?;
        let mut parse_message = message_parser::build_message_parser()?;

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

fn remove_ending_newline(mut event: Event) -> Event {
    if let Some(Value::Bytes(msg)) = event
        .as_mut_log()
        .get_mut(&event::log_schema().message_key())
    {
        if msg.ends_with(&['\n' as u8]) {
            msg.truncate(msg.len() - 1);
        }
    }
    event
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
    // TODO: rename to pod_uid?
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
