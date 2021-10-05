use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
    http::{Auth, HttpClient, HttpError, MaybeAuth},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{BatchedHttpSink, HttpRetryLogic, HttpSink},
        retries::{RetryAction, RetryLogic},
        sink, BatchConfig, BatchSettings, Buffer, Compression, TowerRequestConfig, UriSerde,
    },
    tls::{TlsOptions, TlsSettings},
};
use bytes::Bytes;
use futures::{future, FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use hyper::Body;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

//TODO: add config properties:
// region
// license_key
// api
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct NewRelicConfig {
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

inventory::submit! {
    SinkDescription::new::<NewRelicConfig>("new_relic")
}

impl_generate_config_from_default!(NewRelicConfig);

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[async_trait::async_trait]
#[typetag::serde(name = "new_relic")]
impl SinkConfig for NewRelicConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let batch = BatchSettings::default()
            .bytes(bytesize::mib(10u64))
            .timeout(1)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            Buffer::new(batch.size, self.compression),
            request,
            batch.timeout,
            client.clone(),
            cx.acker()
        )
        .sink_map_err(|error| error!(message = "Fatal new_relic sink error.", %error));

        Ok((
            super::VectorSink::Sink(Box::new(sink)),
            future::ok(()).boxed()
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "new_relic"
    }
}

#[async_trait::async_trait]
impl HttpSink for NewRelicConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        //TODO: check that timestamp has the correct format and reformat if not
        //TODO: For Events, check that eventType exist, set default value if not ("VectorSink")
        //TODO: For Metrics, check name and valu exist and has correct type. Also check type has a valid value if exist
        //TODO: For metrics, remove host, message and source_type
        let log = event.into_log();

        let field = crate::config::log_schema().message_key();
        println!("----------> Get field {}", field);
        let message = log.get(field).expect("Message field not found");
        let message = message.to_string_lossy();

        println!("Message is = {:#?}", message);

        let mut body = serde_json::to_vec(&log).expect("Events should be valid json!");
        body.push(b'\n');

        Some(body)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let uri = "http://localhost:8888".parse::<Uri>().expect("Unable to encode uri");

        let mut builder = Request::post(&uri).header("Content-Type", "application/json");

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        let mut request = builder.body(events).unwrap();

        Ok(request)
    }
}
