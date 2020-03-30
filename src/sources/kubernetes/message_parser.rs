use super::ApplicableTransform;
use crate::{
    event::{self, Event, Value},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        regex_parser::{RegexParser, RegexParserConfig},
        Transform,
    },
};
use chrono::{DateTime, Utc};
use string_cache::DefaultAtom as Atom;

/// Determines format of message.
/// This exists because Docker is still a special entity in Kubernetes as it can write in Json
/// despite CRI defining it's own format.
pub fn build_message_parser() -> crate::Result<ApplicableTransform> {
    let transforms = vec![
        Box::new(DockerMessageTransformer::new()) as Box<dyn Transform>,
        transform_cri_message()?,
    ];
    Ok(ApplicableTransform::Candidates(transforms))
}

#[derive(Debug)]
struct DockerMessageTransformer {
    json_parser: JsonParser,
    atom_time: Atom,
    atom_log: Atom,
}

impl DockerMessageTransformer {
    fn new() -> Self {
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
                Ok(timestamp) => {
                    log.insert(
                        event::log_schema().timestamp_key(),
                        timestamp.with_timezone(&Utc),
                    );
                }
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
            log.insert(event::log_schema().message_key(), message);
        } else {
            debug!(message = "Missing field.", field = %self.atom_log, rate_limit_secs = 10);
            return None;
        }

        Some(event)
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
    fn cri_message_transform() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert(
            "message",
            "2019-10-02T13:21:36.927620189+02:00 stdout F 12".to_owned(),
        );

        let mut transform = transform_cri_message().unwrap();

        let event = transform.transform(event).expect("Transformed");

        has(&event, event::log_schema().message_key(), "12");
        has(&event, "multiline_tag", "F");
        has(&event, "stream", "stdout");
        has(
            &event,
            event::log_schema().timestamp_key(),
            DateTime::parse_from_rfc3339("2019-10-02T13:21:36.927620189+02:00")
                .unwrap()
                .with_timezone(&Utc),
        );
    }

    #[test]
    fn docker_message_transform() {
        let mut event = Event::new_empty_log();
        event.as_mut_log().insert(
            "message",
            r#"{"log":"12", "time":"2019-10-02T13:21:36.927620189+02:00", "stream" : "stdout"}"#
                .to_owned(),
        );

        let mut transform = DockerMessageTransformer::new();

        let event = transform.transform(event).expect("Transformed");

        has(&event, event::log_schema().message_key(), "12");
        has(&event, "stream", "stdout");
        has(
            &event,
            event::log_schema().timestamp_key(),
            DateTime::parse_from_rfc3339("2019-10-02T13:21:36.927620189+02:00")
                .unwrap()
                .with_timezone(&Utc),
        );
    }
}
