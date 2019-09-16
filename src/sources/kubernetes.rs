use crate::{
    event::{self, Event, ValueKind},
    sources::file::{FileConfig, FingerprintingConfig},
    topology::config::{DataType, GlobalOptions, SourceConfig, TransformConfig},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        regex_parser::RegexParserConfig,
        Transform,
    },
};
use chrono::{DateTime, Utc};
use futures::{sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

// ?NOTE
// Original proposal: https://github.com/kubernetes/kubernetes/blob/release-1.5/docs/proposals/kubelet-cri-logging.md#proposed-solution
// Current version: https://github.com/kubernetes/kubernetes/tree/master/staging/src/k8s.io/cri-api/pkg/apis/runtime/v1alpha2
// LogDirectory = `/var/log/pods/<podUID>/`
// LogPath = `containerName/Instance#.log`

/// Location in which by Kubernetes CRI, container runtimes are to store logs.
const LOG_DIRECTORY: &'static str = r"/var/log/pods/";

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
        // Kubernetes source uses 'file source' and various transforms to implement
        // gathering of logs over Kubernetes CRI supported container runtimes.
        // Usability/generality is the goal of this implementation. Performance will
        // be solved with per container runtime specialized source.

        // Side goal is to make kubernetes source behave as simillarly to docker source
        // as possible to set a default behavior for all container related sources.
        // This will help with interchangeability.

        // Only logs created at, or after this moment are logged.
        let now = Utc::now();

        // Tailling logs are limited to those that can have logs created during or
        // after now.

        // File source
        let (file_recv, file_source) = file_source(name, globals)?;

        // Transforms
        let mut transform_message = transform_message();
        let mut transform_file = transform_file()?;

        // Kubernetes source
        let atom_time = Atom::from("time");
        let atom_log = Atom::from("log");
        let source = file_recv
            .filter_map(move |event| transform_message.transform(event))
            .map(move |mut event| {
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
                        Err(error) => warn!(message="Non rfc3339 timestamp",error=%error),
                    }
                } else {
                    warn!(message = "Missing field", field = %atom_time);
                }

                // log -> message
                if let Some(message) = log.remove(&atom_log) {
                    log.insert_explicit(event::MESSAGE.clone(), message);
                } else {
                    warn!(message = "Missing field", field = %atom_log);
                }

                event
            })
            .filter_map(move |event| {
                // Only logs created at, or after now are logged.
                if let Some(ValueKind::Timestamp(ts)) = event.as_log().get(&event::TIMESTAMP) {
                    if ts >= &now {
                        return Some(event);
                    }
                    trace!(message = "Recieved older log", from = %ts.to_rfc3339());
                }
                None
            })
            .filter_map(move |event| transform_file.transform(event))
            .map(|e| {
                println!("Event_out: {:?}", e);
                e
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
    kube_name: &str,
    globals: &GlobalOptions,
) -> Result<(mpsc::Receiver<Event>, super::Source), String> {
    let mut config = FileConfig::default();
    config
        .include
        .push((LOG_DIRECTORY.to_owned() + r"*/*/*.log").into());
    // TODO: Make this a configurable option for excluding namespaces
    // Exclude whole kube-system namespace
    config
        .exclude
        .push((LOG_DIRECTORY.to_owned() + r"kube-system_*/**").into());
    // Exclude whole logging namespace
    config
        .exclude
        .push((LOG_DIRECTORY.to_owned() + r"logging_*/**").into());

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

fn transform_message() -> JsonParser {
    let mut config = JsonParserConfig::default();

    // Don't drop if this is not json to allow user to have some options.
    config.drop_invalid = false;

    // In case this is json, message will be overwritten with log field.
    // In other cases it's better to retain the field. This will allow
    // the user to have some options in how to deal with this.
    config.drop_field = false;

    config.into()
}

fn transform_file() -> Result<Box<dyn Transform>, String> {
    let mut config = RegexParserConfig::default();

    config.field = Some("file".into());

    config.regex = r"^".to_owned()
        + LOG_DIRECTORY
        + r"(?P<pod_uid>[^/]*)/(?P<container_name>[^/]*)/[0-9]*[.]log$";

    // this field is implementation depended so remove it
    config.drop_field = true;

    // pod_uid is a string
    // container_name is a string
    config.build().map_err(|e| {
        format!(
            "Failed in creating file regex transform with error: {:?}",
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_transform() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert_explicit("file".into(),"/var/log/pods/default_busybox-echo-5bdc7bfd99-m996l_e2782fb0-ba64-4289-acd5-68c4f5b0d27e/busybox/3.log".to_owned().into());

        let mut transform = transform_file().unwrap();

        assert_eq!(
            transform
                .transform(event)
                .expect("Transformed")
                .as_log()
                .get(&"container_name".into())
                .expect("container_name present"),
            &ValueKind::from("busybox")
        );
    }
}
