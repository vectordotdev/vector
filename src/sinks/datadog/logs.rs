use crate::{
    event::{log_schema, Event},
    sinks::util::{
        self,
        encoding::{EncodingConfig, EncodingConfiguration},
        tcp::{tcp_healthcheck, TcpSink},
        Encoding, UriSerde,
    },
    tls::{MaybeTlsSettings, TlsConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures01::{stream::iter_ok, Sink};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    endpoint: Option<UriSerde>,
    api_key: String,
    encoding: EncodingConfig<Encoding>,
    tls: Option<TlsConfig>,
}

inventory::submit! {
    SinkDescription::new_without_default::<DatadogLogsConfig>("datadog_logs")
}

#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let (host, port, tls) = if let Some(uri) = &self.endpoint {
            let host = uri
                .host()
                .ok_or_else(|| "A host is required for endpoints".to_string())?;
            let port = uri
                .port_u16()
                .ok_or_else(|| "A port is required for endpoints".to_string())?;

            (format!("{}", host), port, self.tls.clone())
        } else {
            let tls = self.tls.clone().unwrap_or({
                let mut tls = TlsConfig::default();
                tls.enabled = Some(true);
                tls
            });

            ("intake.logs.datadoghq.com".to_string(), 10516, Some(tls))
        };

        let tls_settings = MaybeTlsSettings::from_config(&tls, false)?;

        let sink = TcpSink::new(host.clone(), port, cx.resolver(), tls_settings);
        let healthcheck = tcp_healthcheck(host.clone(), port, cx.resolver());

        let encoding = self.encoding.clone();
        let api_key = Bytes::from(format!("{} ", self.api_key));

        let sink =
            sink.with_flat_map(move |e| iter_ok(encode_event(e, api_key.clone(), &encoding)));

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_logs"
    }
}

fn encode_event(
    mut event: Event,
    mut api_key: Bytes,
    encoding: &EncodingConfig<Encoding>,
) -> Option<Bytes> {
    encoding.apply_rules(&mut event);

    let log = event.as_mut_log();

    if let Some(message) = log.remove(&log_schema().message_key()) {
        log.insert("message", message);
    }

    if let Some(timestamp) = log.remove(&log_schema().timestamp_key()) {
        log.insert("date", timestamp);
    }

    if let Some(host) = log.remove(&log_schema().host_key()) {
        log.insert("host", host);
    }

    if let Some(bytes) = util::encode_event(event, encoding) {
        // Prepend the api_key:
        // {API_KEY} {EVENT_BYTES}
        api_key.extend(bytes);

        Some(api_key)
    } else {
        None
    }
}
