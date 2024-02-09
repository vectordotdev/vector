use crate::decoding::format::Deserializer;
use crate::BytesDeserializerConfig;
use bytes::Bytes;
use derivative::Derivative;
use smallvec::{smallvec, SmallVec};
use vector_config_macros::configurable_component;
use vector_core::config::{DataType, LogNamespace};
use vector_core::event::{Event, TargetEvents, VrlTarget};
use vector_core::{compile_vrl, schema};
use vrl::compiler::state::ExternalEnv;
use vrl::compiler::{runtime::Runtime, CompileConfig, Program, TimeZone, TypeState};
use vrl::diagnostic::Formatter;
use vrl::value::Kind;

/// Config used to build a `VrlDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct VrlDeserializerConfig {
    /// VRL-specific decoding options.
    vrl: VrlDeserializerOptions,
}

/// VRL-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
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
    /// If not set, `local` will be used.
    ///
    /// [tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    pub timezone: Option<TimeZone>,
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
            &vrl::stdlib::all(),
            &state,
            CompileConfig::default(),
        ) {
            Ok(result) => Ok(VrlDeserializer {
                program: result.program,
                timezone: self.vrl.timezone.unwrap_or(TimeZone::Local),
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
}

fn parse_bytes(bytes: Bytes, log_namespace: LogNamespace) -> Event {
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
        let event = parse_bytes(bytes, log_namespace);
        match self.run_vrl(event, log_namespace) {
            Ok(events) => Ok(events),
            Err(e) => Err(e),
        }
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
    use super::*;
    use chrono::{DateTime, Utc};
    use indoc::indoc;
    use vrl::btreemap;
    use vrl::path::OwnedTargetPath;
    use vrl::value::Value;

    fn make_decoder(source: &str) -> VrlDeserializer {
        VrlDeserializerConfig {
            vrl: VrlDeserializerOptions {
                source: source.to_string(),
                timezone: None,
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
        let cef_bytes = Bytes::from("CEF:0|Security|Threat Manager|1.0|100|worm successfully stopped|10|src=10.0.0.1 dst=2.1.2.2 spt=1232");
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
}
