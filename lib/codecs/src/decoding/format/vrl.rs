use bytes::Bytes;
use smallvec::{SmallVec, smallvec};
use vector_config_macros::configurable_component;
use vector_core::{
    compile_vrl,
    config::{DataType, LogNamespace},
    event::{Event, TargetEvents, VrlTarget},
    schema,
};
use vrl::{
    compiler::{CompileConfig, Program, TimeZone, TypeState, runtime::Runtime, state::ExternalEnv},
    diagnostic::Formatter,
    value::Kind,
};

use vector_core::event::EventMetadata;

use crate::decoding::format::Deserializer;

/// Config used to build a `VrlDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct VrlDeserializerConfig {
    /// VRL-specific decoding options.
    pub vrl: VrlDeserializerOptions,
}

/// VRL-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VrlDeserializerOptions {
    /// The [Vector Remap Language][vrl] (VRL) program to execute for each event.
    /// Note that the final contents of the `.` target will be used as the decoding result.
    /// Compilation error or use of 'abort' in a program will result in a decoding error.
    ///
    ///
    /// [vrl]: https://vector.dev/docs/reference/vrl
    pub source: String,

    /// The name of the timezone to apply to timestamp conversions that do not contain an explicit
    /// time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
    /// to indicate system local time.
    ///
    /// If not set, `local` is used.
    ///
    /// [tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    pub timezone: Option<TimeZone>,

    /// When `true`, the source may inject per-request metadata into the VRL
    /// runtime before the program executes. Injected metadata is accessible
    /// via `%`-prefixed paths (e.g. `%exec.host`, `%vector.secrets.*`).
    ///
    /// Each source controls which metadata it injects; see the source
    /// documentation for details. If the source does not support metadata
    /// injection, this option has no effect.
    #[serde(default)]
    pub inject_metadata: bool,
}

impl VrlDeserializerConfig {
    /// Build the `VrlDeserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<VrlDeserializer> {
        let state = TypeState {
            local: Default::default(),
            external: ExternalEnv::default(),
        };

        match compile_vrl(
            &self.vrl.source,
            &vector_vrl_functions::all(),
            &state,
            CompileConfig::default(),
        ) {
            Ok(result) => Ok(VrlDeserializer {
                program: result.program,
                timezone: self.vrl.timezone.unwrap_or(TimeZone::Local),
                inject_metadata_enabled: self.vrl.inject_metadata,
                metadata_template: None,
            }),
            Err(diagnostics) => Err(Formatter::new(&self.vrl.source, diagnostics)
                .to_string()
                .into()),
        }
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                schema::Definition::empty_legacy_namespace().unknown_fields(Kind::any())
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            }
        }
    }
}

/// Deserializer that builds `Event`s from a byte frame containing logs compatible with VRL.
#[derive(Debug, Clone)]
pub struct VrlDeserializer {
    program: Program,
    timezone: TimeZone,
    /// Whether this deserializer accepts a metadata template from its source.
    /// Set from [`VrlDeserializerOptions::inject_metadata`] at build time.
    inject_metadata_enabled: bool,
    /// Per-call metadata template. Only populated when `inject_metadata_enabled`
    /// is true and the source calls [`VrlDeserializer::with_metadata_template`].
    metadata_template: Option<EventMetadata>,
}

impl VrlDeserializer {
    /// Attach a metadata template that will be pre-populated on each synthetic
    /// event before the VRL program runs. This is a no-op unless
    /// `inject_metadata: true` was set in the VRL decoder config.
    ///
    /// Sources call this once per request/frame with the metadata they have
    /// assembled (e.g. envelope fields, auth tokens). VRL can then read those
    /// values via `%`-prefixed paths such as `%exec.host` or
    /// `%vector.secrets.*`.
    #[must_use]
    pub fn with_metadata_template(mut self, metadata: EventMetadata) -> Self {
        if self.inject_metadata_enabled {
            self.metadata_template = Some(metadata);
        }
        self
    }
}

fn parse_bytes(bytes: Bytes, log_namespace: LogNamespace) -> Event {
    use crate::BytesDeserializerConfig;
    let bytes_deserializer = BytesDeserializerConfig::new().build();
    let log_event = bytes_deserializer.parse_single(bytes, log_namespace);
    Event::from(log_event)
}

impl Deserializer for VrlDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let mut event = parse_bytes(bytes, log_namespace);
        if let Some(template) = &self.metadata_template {
            // Pre-populate the synthetic event with the source-assembled metadata so
            // every `%`-prefixed path is in scope when VRL executes. This lets
            // user programs read `%splunk_hec.host`, `%vector.secrets.*`, etc.
            *event.metadata_mut() = template.clone();
        }
        self.run_vrl(event, log_namespace)
    }
}

impl VrlDeserializer {
    fn run_vrl(
        &self,
        event: Event,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let mut runtime = Runtime::default();
        let mut target = VrlTarget::new(event, self.program.info(), true);
        match runtime.resolve(&mut target, &self.program, &self.timezone) {
            Ok(_) => match target.into_events(log_namespace) {
                TargetEvents::One(event) => Ok(smallvec![event]),
                TargetEvents::Logs(events_iter) => Ok(SmallVec::from_iter(events_iter)),
                TargetEvents::Traces(_) => Err("trace targets are not supported".into()),
            },
            Err(e) => Err(e.to_string().into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use indoc::indoc;
    use vrl::{btreemap, path::OwnedTargetPath, value::Value};

    use super::*;

    fn make_decoder(source: &str) -> VrlDeserializer {
        VrlDeserializerConfig {
            vrl: VrlDeserializerOptions {
                source: source.to_string(),
                timezone: None,
                inject_metadata: false,
            },
        }
        .build()
        .expect("Failed to build VrlDeserializer")
    }

    fn make_decoder_with_inject_metadata(source: &str) -> VrlDeserializer {
        VrlDeserializerConfig {
            vrl: VrlDeserializerOptions {
                source: source.to_string(),
                timezone: None,
                inject_metadata: true,
            },
        }
        .build()
        .expect("Failed to build VrlDeserializer")
    }

    #[test]
    fn test_json_message() {
        let source = indoc!(
            r#"
            %m1 = "metadata"
            . = string!(.)
            . = parse_json!(.)
            "#
        );

        let decoder = make_decoder(source);

        let log_bytes = Bytes::from(r#"{ "message": "Hello VRL" }"#);
        let result = decoder.parse(log_bytes, LogNamespace::Vector).unwrap();
        assert_eq!(result.len(), 1);
        let event = result.first().unwrap();
        assert_eq!(
            *event.as_log().get(&OwnedTargetPath::event_root()).unwrap(),
            btreemap! { "message" => "Hello VRL" }.into()
        );
        assert_eq!(
            *event
                .as_log()
                .get(&OwnedTargetPath::metadata_root())
                .unwrap(),
            btreemap! { "m1" => "metadata" }.into()
        );
    }

    #[test]
    fn test_ignored_returned_expression() {
        let source = indoc!(
            r#"
            . = { "a" : 1 }
            { "b" : 9 }
        "#
        );

        let decoder = make_decoder(source);

        let log_bytes = Bytes::from("some bytes");
        let result = decoder.parse(log_bytes, LogNamespace::Vector).unwrap();
        assert_eq!(result.len(), 1);
        let event = result.first().unwrap();
        assert_eq!(
            *event.as_log().get(&OwnedTargetPath::event_root()).unwrap(),
            btreemap! { "a" => 1 }.into()
        );
    }

    #[test]
    fn test_multiple_events() {
        let source = indoc!(". = [0,1,2]");
        let decoder = make_decoder(source);
        let log_bytes = Bytes::from("some bytes");
        let result = decoder.parse(log_bytes, LogNamespace::Vector).unwrap();
        assert_eq!(result.len(), 3);
        for (i, event) in result.iter().enumerate() {
            assert_eq!(
                *event.as_log().get(&OwnedTargetPath::event_root()).unwrap(),
                i.into()
            );
        }
    }

    #[test]
    fn test_syslog_and_cef_input() {
        let source = indoc!(
            r#"
            if exists(.message) {
                . = string!(.message)
            }
            . = parse_syslog(.) ?? parse_cef(.) ?? null
            "#
        );

        let decoder = make_decoder(source);

        // Syslog input
        let syslog_bytes = Bytes::from(
            "<34>1 2024-02-06T15:04:05.000Z mymachine.example.com su - ID47 - 'su root' failed for user on /dev/pts/8",
        );
        let result = decoder.parse(syslog_bytes, LogNamespace::Vector).unwrap();
        assert_eq!(result.len(), 1);
        let syslog_event = result.first().unwrap();
        assert_eq!(
            *syslog_event
                .as_log()
                .get(&OwnedTargetPath::event_root())
                .unwrap(),
            btreemap! {
                "appname" => "su",
                "facility" => "auth",
                "hostname" => "mymachine.example.com",
                "message" => "'su root' failed for user on /dev/pts/8",
                "msgid" => "ID47",
                "severity" => "crit",
                "timestamp" => "2024-02-06T15:04:05Z".parse::<DateTime<Utc>>().unwrap(),
                "version" => 1
            }
            .into()
        );

        // CEF input
        let cef_bytes = Bytes::from(
            "CEF:0|Security|Threat Manager|1.0|100|worm successfully stopped|10|src=10.0.0.1 dst=2.1.2.2 spt=1232",
        );
        let result = decoder.parse(cef_bytes, LogNamespace::Vector).unwrap();
        assert_eq!(result.len(), 1);
        let cef_event = result.first().unwrap();
        assert_eq!(
            *cef_event
                .as_log()
                .get(&OwnedTargetPath::event_root())
                .unwrap(),
            btreemap! {
                "cefVersion" =>"0",
                "deviceEventClassId" =>"100",
                "deviceProduct" =>"Threat Manager",
                "deviceVendor" =>"Security",
                "deviceVersion" =>"1.0",
                "dst" =>"2.1.2.2",
                "name" =>"worm successfully stopped",
                "severity" =>"10",
                "spt" =>"1232",
                "src" =>"10.0.0.1"
            }
            .into()
        );
        let random_bytes = Bytes::from("a|- -| x");
        let result = decoder.parse(random_bytes, LogNamespace::Vector).unwrap();
        let random_event = result.first().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            *random_event
                .as_log()
                .get(&OwnedTargetPath::event_root())
                .unwrap(),
            Value::Null
        );
    }

    #[test]
    fn test_invalid_source() {
        let error = VrlDeserializerConfig {
            vrl: VrlDeserializerOptions {
                source: ". ?".to_string(),
                timezone: None,
                inject_metadata: false,
            },
        }
        .build()
        .unwrap_err()
        .to_string();
        assert!(error.contains("error[E203]: syntax error"));
    }

    #[test]
    fn test_abort() {
        let decoder = make_decoder("abort");
        let log_bytes = Bytes::from(r#"{ "message": "Hello VRL" }"#);
        let error = decoder
            .parse(log_bytes, LogNamespace::Vector)
            .unwrap_err()
            .to_string();
        assert!(error.contains("aborted"));
    }

    // Tests for `with_metadata_template` —————————————————————————————————————

    fn metadata_with_secret(key: &str, value: &str) -> EventMetadata {
        let mut metadata = EventMetadata::default();
        metadata.secrets_mut().insert(key, value);
        metadata
    }

    /// A VRL program that uses `get_secret!()` can read a secret injected via
    /// `with_metadata_template`.
    #[test]
    fn test_with_metadata_template_vrl_can_read_secret() {
        // VRL program copies the injected secret into an event field so we can
        // assert on its value. The input bytes become `.message` (Legacy namespace)
        // and we add `.secret_value` alongside it.
        let decoder = make_decoder_with_inject_metadata(r#".secret_value = get_secret!("my_token")"#)
            .with_metadata_template(metadata_with_secret("my_token", "super-secret"));

        let bytes = Bytes::from(r#"hello"#);
        let events = decoder
            .parse(bytes, LogNamespace::Legacy)
            .expect("parse should succeed");

        assert_eq!(events.len(), 1);
        assert_eq!(
            *events[0].as_log().get("secret_value").unwrap(),
            Value::from("super-secret")
        );
    }

    /// Secrets explicitly set by the VRL program win over the template because
    /// `set_secret!` runs after the template is pre-populated.
    #[test]
    fn test_with_metadata_template_codec_wins_on_collision() {
        let decoder =
            make_decoder_with_inject_metadata(r#"set_secret!("my_token", "codec-wins")"#)
                .with_metadata_template(metadata_with_secret("my_token", "template-loses"));

        let bytes = Bytes::from(r#"hello"#);
        let events = decoder
            .parse(bytes, LogNamespace::Legacy)
            .expect("parse should succeed");

        assert_eq!(
            events[0].metadata().secrets().get("my_token").unwrap().as_ref(),
            "codec-wins"
        );
    }
}
