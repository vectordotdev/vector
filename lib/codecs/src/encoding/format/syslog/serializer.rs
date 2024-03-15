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
        match event {
            Event::Log(log) => {
                let mut buf = String::from("<");
                let pri = get_num_facility(&self.config.facility, &log) * 8 + get_num_severity(&self.config.severity, &log);
                buf.push_str(&pri.to_string());
                buf.push_str(">");
                match self.config.rfc {
                    SyslogRFC::Rfc3164 => {
                        let timestamp = get_timestamp(&log);
                        let formatted_timestamp = format!(" {} ", timestamp.format("%b %e %H:%M:%S"));
                        buf.push_str(&formatted_timestamp);
                        buf.push_str(&get_field("hostname", &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&self.config.tag, &log));
                        buf.push_str(": ");
                        if self.config.add_log_source {
                            add_log_source(&log, &mut buf);
                        }
                    },
                    SyslogRFC::Rfc5424 => {
                        buf.push_str("1 ");
                        let timestamp = get_timestamp(&log);
                        buf.push_str(&timestamp.to_rfc3339_opts(SecondsFormat::Millis, true));
                        buf.push(' ');
                        buf.push_str(&get_field("hostname", &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.app_name, &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.proc_id, &log));
                        buf.push(' ');
                        buf.push_str(&get_field_or_config(&&self.config.msg_id, &log));
                        buf.push_str(" - "); // no structured data
                        if self.config.add_log_source {
                            add_log_source(&log, &mut buf);
                        }
                    }
                }
                let mut payload = if self.config.payload_key.is_empty() {
                    serde_json::to_vec(&log).unwrap_or_default()
                } else {
                    get_field(&&self.config.payload_key, &log).as_bytes().to_vec()
                };
                let mut vec = buf.as_bytes().to_vec();
                vec.append(&mut payload);
                buffer.put_slice(&vec);
            },
            _ => {}
        }
        Ok(())
    }
}

fn get_field_or_config(config_name: &String, log: &LogEvent) -> String {
    if let Some(field_name) = config_name.strip_prefix("$.message.") {
        return get_field(field_name, log)
    } else {
        return config_name.clone()
    }
}

fn get_field(field_name: &str, log: &LogEvent) -> String {
    if let Some(field_value) = log.get(field_name) {
        return String::from_utf8(field_value.coerce_to_bytes().to_vec()).unwrap_or_default();
    } else {
        return NIL_VALUE.to_string()
    }
}

fn get_timestamp(log: &LogEvent) -> DateTime::<Local> {
    match log.get("@timestamp") {
        Some(value) => {
            if let Value::Timestamp(timestamp) = value {
                DateTime::<Local>::from(*timestamp)
            } else {
                Local::now()
            }
        },
        _ => Local::now()
    }
}

fn add_log_source(log: &LogEvent, buf: &mut String) {
    buf.push_str("namespace_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.namespace_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", container_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.container_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", pod_name=");
    buf.push_str(&String::from_utf8(
        log
        .get("kubernetes.pod_name")
        .map(|h| h.coerce_to_bytes())
        .unwrap_or_default().to_vec()
    ).unwrap());
    buf.push_str(", message=");
}
