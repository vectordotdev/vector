/// Serializer that converts an `Event` to bytes using the Syslog format.
#[derive(Debug, Clone)]
pub struct SyslogSerializer {
    config: SyslogSerializerConfig
}

impl SyslogSerializer {
    /// Creates a new `SyslogSerializer`.
    pub fn new(conf: &SyslogSerializerConfig) -> Self {
        Self { config: conf.clone() }
    }
}

impl Encoder<Event> for SyslogSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if let Event::Log(log_event) = event {
            let syslog_message = ConfigDecanter::new(log_event).decant_config(&self.config);

            let vec = syslog_message
                .encode(&self.config.rfc)
                .as_bytes()
                .to_vec();
            buffer.put_slice(&vec);
        }

        Ok(())
    }
}

// Adapts a `LogEvent` into a `SyslogMessage` based on config from `SyslogSerializerConfig`:
// - Splits off the responsibility of encoding logic to `SyslogMessage` (which is not dependent upon Vector types).
// - Majority of methods are only needed to support the `decant_config()` operation.
struct ConfigDecanter {
    log: LogEvent,
}

impl ConfigDecanter {
    fn new(log: LogEvent) -> Self {
        Self {
            log,
        }
    }

    fn decant_config(&self, config: &SyslogSerializerConfig) -> SyslogMessage {
        let x = |v| self.replace_if_proxied(v).unwrap_or_default();
        let facility = x(&config.facility);
        let severity = x(&config.severity);

        let y = |v| self.replace_if_proxied_opt(v);
        let app_name = y(&config.app_name).unwrap_or("vector".to_owned());
        let proc_id = y(&config.proc_id);
        let msg_id = y(&config.msg_id);

        SyslogMessage {
            pri: Pri::from_str_variants(&facility, &severity),
            timestamp: self.get_timestamp(),
            hostname: self.value_by_key("hostname"),
            tag: Tag {
                app_name,
                proc_id,
                msg_id,
            },
            structured_data: None,
            message: self.get_message(&config),
        }
    }

    fn replace_if_proxied_opt(&self, value: &Option<String>) -> Option<String> {
        value.as_ref().and_then(|v| self.replace_if_proxied(v))
    }

    // When the value has the expected prefix, perform a lookup for a field key without that prefix part.
    // A failed lookup returns `None`, while a value without the prefix uses the config value as-is.
    //
    // Q: Why `$.message.` as the prefix? (Appears to be JSONPath syntax?)
    // NOTE: Originally named in PR as: `get_field_or_config()`
    fn replace_if_proxied(&self, value: &str) -> Option<String> {
        value
            .strip_prefix("$.message.")
            .map_or(
                Some(value.to_owned()),
                |field_key| self.value_by_key(field_key),
            )
    }

    // NOTE: Originally named in PR as: `get_field()`
    // Now returns a `None` directly instead of converting to either `"-"` or `""`
    fn value_by_key(&self, field_key: &str) -> Option<String> {
        self.log.get(field_key).and_then(|field_value| {
            let bytes = field_value.coerce_to_bytes();
            String::from_utf8(bytes.to_vec()).ok()
        })
    }

    fn get_timestamp(&self) -> DateTime::<Local> {
        // Q: Was this Timestamp key hard-coded to the needs of the original PR author?
        //
        // Key `@timestamp` depends on input:
        // https://vector.dev/guides/level-up/managing-schemas/#example-custom-timestamp-field
        // https://vector.dev/docs/about/under-the-hood/architecture/data-model/log/#timestamps
        // NOTE: Log schema key renaming is unavailable when Log namespacing is enabled:
        // https://vector.dev/docs/reference/configuration/global-options/#log_schema
        //
        // NOTE: Log namespacing has metadata `%vector.ingest_timestamp` from a source (file/demo_logs) instead of `timestamp`.
        // As a `payload_key` it will not respect config `encoding.timestamp_format`, but does when
        // using the parent object (`%vector`). Inputs without namespacing respect that config setting.
        if let Some(Value::Timestamp(timestamp)) = self.log.get("@timestamp") {
            // Q: Utc type returned is changed to Local?
            // - Could otherwise return `*timestamp` as-is? Why is Local conversion necessary?
            DateTime::<Local>::from(*timestamp)
        } else {
            // NOTE: Local time is encouraged by RFC 5424 when creating a fallback timestamp for RFC 3164
            Local::now()
        }
    }

    fn get_message(&self, config: &SyslogSerializerConfig) -> String {
        let mut message = String::new();

        if config.add_log_source {
            message.push_str(self.add_log_source().as_str());
        }

        // `payload_key` configures where to source the value for the syslog `message`:
        // - Field key (Valid)   => Get value by lookup (value_by_key)
        // - Field key (Invalid) => Empty string (unwrap_or_default)
        // - Not configured      => JSON encoded `LogEvent` (fallback?)
        //
        // Q: Was the JSON fallback intended by the original PR author only for debugging?
        //    Roughly equivalent to using `payload_key: .` (in YAML config)?
        let payload = if config.payload_key.is_empty() {
            serde_json::to_string(&self.log).ok()
        } else {
            self.value_by_key(&config.payload_key)
        };

        message.push_str(&payload.unwrap_or_default());
        message
    }

    // NOTE: This is a third-party addition from the original PR author (it is not relevant to the syslog spec):
    // TODO: Remove, as this type of additional data is better supported via VRL remap + `StructuredData`?
    fn add_log_source(&self) -> String {
        let get_value = |s| self.value_by_key(s).unwrap_or_default();

        [
            "namespace_name=", get_value("kubernetes.namespace_name").as_str(),
            ", container_name=", get_value("kubernetes.container_name").as_str(),
            ", pod_name=", get_value("kubernetes.pod_name").as_str(),
            ", message="
        ].concat()
    }
}
