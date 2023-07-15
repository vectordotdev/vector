use http::{Request, StatusCode, Uri};
use hyper::Body;

use super::{
    service::{ClickhouseRetryLogic, ClickhouseService},
    sink::ClickhouseSink,
};
use crate::{
    http::{get_http_scheme_from_uri, Auth, HttpClient, MaybeAuth},
    sinks::{
        prelude::*,
        util::{RealtimeSizeBasedDefaultBatchSettings, UriSerde},
    },
};

/// Configuration for the `clickhouse` sink.
#[configurable_component(sink("clickhouse", "Deliver log data to a ClickHouse database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    /// The endpoint of the ClickHouse server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://localhost:8123"))]
    pub endpoint: UriSerde,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    /// The database that contains the table that data is inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<String>,

    /// Sets `input_format_skip_unknown_fields`, allowing ClickHouse to discard fields not present in the table schema.
    #[serde(default)]
    pub skip_unknown_fields: bool,

    /// Sets `date_time_input_format` to `best_effort`, allowing ClickHouse to properly parse RFC3339/ISO 8601.
    #[serde(default)]
    pub date_time_best_effort: bool,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(ClickhouseConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "clickhouse")]
impl SinkConfig for ClickhouseConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = self.endpoint.with_default_parts().uri;
        let protocol = get_http_scheme_from_uri(&endpoint);

        let auth = self.auth.choose_one(&self.endpoint.auth)?;

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let service = ClickhouseService::new(
            client.clone(),
            auth.clone(),
            &endpoint,
            self.database.as_deref(),
            self.table.as_str(),
            self.skip_unknown_fields,
            self.date_time_best_effort,
        )?;

        let request_limits = self.request.unwrap_with(&Default::default());
        let service = ServiceBuilder::new()
            .settings(request_limits, ClickhouseRetryLogic::default())
            .service(service);

        let batch_settings = self.batch.into_batcher_settings()?;
        let sink = ClickhouseSink::new(
            batch_settings,
            self.compression,
            self.encoding.clone(),
            service,
            protocol,
        );

        let healthcheck = Box::pin(healthcheck(client, endpoint, auth));

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck(client: HttpClient, endpoint: Uri, auth: Option<Auth>) -> crate::Result<()> {
    // TODO: check if table exists?
    let uri = format!("{}/?query=SELECT%201", endpoint);
    let mut request = Request::get(uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ClickhouseConfig>();
    }
}
