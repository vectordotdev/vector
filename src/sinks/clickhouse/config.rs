//! Configuration for the `Clickhouse` sink.

use futures_util::TryFutureExt;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use std::fmt;
use vector_lib::codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer};

use super::{
    request_builder::ClickhouseRequestBuilder,
    service::{ClickhouseHealthLogic, ClickhouseRetryLogic, ClickhouseServiceRequestBuilder},
    sink::{ClickhouseSink, PartitionKey},
};
use crate::{
    dns,
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        prelude::*,
        util::{RealtimeSizeBasedDefaultBatchSettings, UriSerde, http::HttpService},
    },
};

/// Data format.
///
/// The format used to parse input/output data.
///
/// [formats]: https://clickhouse.com/docs/en/interfaces/formats
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
#[allow(clippy::enum_variant_names)]
pub enum Format {
    #[derivative(Default)]
    /// JSONEachRow.
    JsonEachRow,

    /// JSONAsObject.
    JsonAsObject,

    /// JSONAsString.
    JsonAsString,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::JsonEachRow => write!(f, "JSONEachRow"),
            Format::JsonAsObject => write!(f, "JSONAsObject"),
            Format::JsonAsString => write!(f, "JSONAsString"),
        }
    }
}

/// Configuration for the `clickhouse` sink.
#[configurable_component(sink("clickhouse", "Deliver log data to a ClickHouse database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    /// The endpoint of the ClickHouse server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://localhost:8123"))]
    pub endpoint: UriSerde,

    /// Automatically resolve hostnames to all available IP addresses.
    ///
    /// When enabled, the hostname in the endpoint will be resolved to all its IP addresses,
    /// and Vector will load balance across all resolved IPs.
    #[serde(default)]
    pub auto_resolve_dns: bool,

    /// The table that data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: Template,

    /// The database that contains the table that data is inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<Template>,

    /// The format to parse input data.
    #[serde(default)]
    pub format: Format,

    /// Sets `input_format_skip_unknown_fields`, allowing ClickHouse to discard fields not present in the table schema.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub skip_unknown_fields: Option<bool>,

    /// Sets `date_time_input_format` to `best_effort`, allowing ClickHouse to properly parse RFC3339/ISO 8601.
    #[serde(default)]
    pub date_time_best_effort: bool,

    /// Sets `insert_distributed_one_random_shard`, allowing ClickHouse to insert data into a random shard when using Distributed Table Engine.
    #[serde(default)]
    pub insert_random_shard: bool,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
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
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub query_settings: QuerySettingsConfig,
}

/// Query settings for the `clickhouse` sink.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct QuerySettingsConfig {
    /// Async insert-related settings.
    #[serde(default)]
    pub async_insert_settings: AsyncInsertSettingsConfig,
}

/// Async insert related settings for the `clickhouse` sink.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct AsyncInsertSettingsConfig {
    /// Sets `async_insert`, allowing ClickHouse to queue the inserted data and later flush to table in the background.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Sets `wait_for`, allowing ClickHouse to wait for processing of asynchronous insertion.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub wait_for_processing: Option<bool>,

    /// Sets 'wait_for_processing_timeout`, to control the timeout for waiting for processing asynchronous insertion.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub wait_for_processing_timeout: Option<u64>,

    /// Sets `async_insert_deduplicate`, allowing ClickHouse to perform deduplication when inserting blocks in the replicated table.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub deduplicate: Option<bool>,

    /// Sets `async_insert_max_data_size`, the maximum size in bytes of unparsed data collected per query before being inserted.
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub max_data_size: Option<u64>,

    /// Sets `async_insert_max_query_number`, the maximum number of insert queries before being inserted
    ///
    /// If left unspecified, use the default provided by the `ClickHouse` server.
    #[serde(default)]
    pub max_query_number: Option<u64>,
}

impl_generate_config_from_default!(ClickhouseConfig);

#[derive(Debug, Clone)]
pub struct ClickhouseCommon {
    pub endpoint: Uri,
    pub auth: Option<Auth>,
    pub tls_settings: TlsSettings,
    service_request_builder: ClickhouseServiceRequestBuilder,
}

impl ClickhouseCommon {
    pub async fn parse_config(
        config: &ClickhouseConfig,
        endpoint_str: &str,
    ) -> crate::Result<Self> {
        let endpoint = endpoint_str.parse::<UriSerde>()?;
        let endpoint_uri = endpoint.with_default_parts().uri;

        let auth = config.auth.choose_one(&endpoint.auth)?;
        let tls_settings = TlsSettings::from_options(config.tls.as_ref())?;

        let service_request_builder = ClickhouseServiceRequestBuilder {
            auth: auth.clone(),
            endpoint: endpoint_uri.clone(),
            skip_unknown_fields: config.skip_unknown_fields,
            date_time_best_effort: config.date_time_best_effort,
            insert_random_shard: config.insert_random_shard,
            compression: config.compression,
            query_settings: config.query_settings,
        };

        Ok(Self {
            endpoint: endpoint_uri,
            auth,
            tls_settings,
            service_request_builder,
        })
    }

    pub async fn parse_many(config: &ClickhouseConfig) -> crate::Result<Vec<Self>> {
        let endpoint_str = config.endpoint.with_default_parts().uri.to_string();

        let all_endpoints = if config.auto_resolve_dns {
            Self::resolve_endpoint_to_ips(&endpoint_str).await?
        } else {
            vec![endpoint_str]
        };

        if all_endpoints.is_empty() {
            return Err("No endpoints available after DNS resolution".into());
        }

        let mut commons = Vec::new();
        for endpoint_str in all_endpoints {
            commons.push(Self::parse_config(config, &endpoint_str).await?);
        }
        Ok(commons)
    }

    async fn resolve_endpoint_to_ips(endpoint_str: &str) -> crate::Result<Vec<String>> {
        let uri: Uri = endpoint_str.parse()?;

        let host = uri.host().ok_or("URI must contain a host")?;

        // Resolve hostname to all IP addresses
        let ips: Vec<_> = dns::Resolver.lookup_ip(host.to_string()).await?.collect();

        if ips.is_empty() {
            return Err("No IP addresses found for hostname".into());
        }

        let mut resolved_endpoints = Vec::new();
        for ip in ips {
            let new_endpoint = uri.to_string().replace(host, &ip.to_string());
            resolved_endpoints.push(new_endpoint);
        }

        Ok(resolved_endpoints)
    }

    pub(super) const fn get_service_request_builder(&self) -> &ClickhouseServiceRequestBuilder {
        &self.service_request_builder
    }

    pub async fn healthcheck(self, client: HttpClient) -> crate::Result<()> {
        let uri = get_healthcheck_uri(&self.endpoint);
        let mut request = Request::get(uri).body(Body::empty()).unwrap();

        if let Some(auth) = self.auth {
            auth.apply(&mut request);
        }

        let response = client.send(request).await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "clickhouse")]
impl SinkConfig for ClickhouseConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let commons = ClickhouseCommon::parse_many(self).await?;
        let common = commons[0].clone();

        let client = HttpClient::new(common.tls_settings.clone(), &cx.proxy)?;

        let request_limits = self.request.into_settings();

        let services = commons
            .iter()
            .map(|common| {
                let endpoint = common.endpoint.to_string();
                let service: HttpService<ClickhouseServiceRequestBuilder, PartitionKey> =
                    HttpService::new(client.clone(), common.get_service_request_builder().clone());
                (endpoint, service)
            })
            .collect::<Vec<_>>();

        let service = request_limits.distributed_service(
            ClickhouseRetryLogic::default(),
            services,
            Default::default(),
            ClickhouseHealthLogic,
            1,
        );

        let batch_settings = self.batch.into_batcher_settings()?;

        let database = self.database.clone().unwrap_or_else(|| {
            "default"
                .try_into()
                .expect("'default' should be a valid template")
        });

        let request_builder = ClickhouseRequestBuilder {
            compression: self.compression,
            encoding: (
                self.encoding.clone(),
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoderConfig.build().into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            ),
        };

        let sink = ClickhouseSink::new(
            batch_settings,
            service,
            database,
            self.table.clone(),
            self.format,
            request_builder,
        );

        let healthcheck = futures::future::select_ok(
            commons
                .into_iter()
                .map(move |common| common.healthcheck(client.clone()).boxed()),
        )
        .map_ok(|((), _)| ())
        .boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

fn get_healthcheck_uri(endpoint: &Uri) -> String {
    let mut uri = endpoint.to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }
    uri.push_str("?query=SELECT%201");
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ClickhouseConfig>();
    }

    #[test]
    fn test_get_healthcheck_uri() {
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123".parse().unwrap()),
            "http://localhost:8123/?query=SELECT%201"
        );
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123/".parse().unwrap()),
            "http://localhost:8123/?query=SELECT%201"
        );
        assert_eq!(
            get_healthcheck_uri(&"http://localhost:8123/path/".parse().unwrap()),
            "http://localhost:8123/path/?query=SELECT%201"
        );
    }

    #[tokio::test]
    async fn test_auto_resolve_dns_enabled() {
        let config = ClickhouseConfig {
            endpoint: "http://localhost:8123".parse().unwrap(),
            auto_resolve_dns: true, // Enabled
            table: "test_table".try_into().unwrap(),
            ..Default::default()
        };

        let commons = ClickhouseCommon::parse_many(&config).await.unwrap();
        assert!(!commons.is_empty());

        // All resolved endpoints should be IP addresses, not hostnames
        for common in &commons {
            let endpoint_str = common.endpoint.to_string();
            assert!(!endpoint_str.contains("localhost"));
            // Should contain either IPv4 or IPv6 addresses
            assert!(endpoint_str.contains("127.0.0.1") || endpoint_str.contains("::1"));
        }
    }
}
