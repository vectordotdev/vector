use std::{collections::BTreeMap, sync::Arc, time::Duration};

use http::{HeaderValue, Method, Uri, header::AUTHORIZATION};

use super::{
    Tenant,
    encoder::{VmEncoder, compress},
    request_builder::VmRequestBuilder,
    retry::VmRetryLogic,
    service::{
        MULTITENANT_WRITE_PATH, RequestSettings, VmService, WRITE_PATH, WriteEndpoint,
        tenant_write_path,
    },
    sink::VmSink,
};
use crate::{
    config::ValidatedSink,
    http::{Auth, HttpClient},
    sinks::{
        prelude::*,
        util::{
            HttpEndpoint,
            http::{OrderedHeaderName, RetryStrategy},
        },
    },
    template::ConfinementConfig,
};

/// Configuration for the `victoriametrics` sink.
#[configurable_component(sink(
    "victoriametrics",
    "Deliver metric data to VictoriaMetrics using the VictoriaMetrics remote write protocol."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VictoriaMetricsConfig {
    /// The base URL of VictoriaMetrics.
    ///
    /// This is the URL of single-node VictoriaMetrics, `vminsert`, or `vmauth`. The sink appends
    /// the write path, which depends on `tenant.mode`.
    #[configurable(metadata(docs::examples = "http://localhost:8428"))]
    #[configurable(metadata(docs::examples = "http://vminsert:8480"))]
    #[configurable(metadata(docs::examples = "http://vmauth:8427"))]
    pub endpoint: HttpEndpoint,

    #[serde(default)]
    pub tenant: TenantConfig,

    /// The default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with an underscore (`_`).
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: Option<String>,

    /// Whether to send metric type metadata.
    ///
    /// VictoriaMetrics stores metadata when `-enableMetadata` is set, which is the default. As in
    /// `vmagent`, metadata is sent by default.
    #[serde(default = "crate::serde::default_true")]
    pub send_metadata: bool,

    /// The zstd compression level.
    ///
    /// Higher levels reduce network traffic at the cost of CPU usage. Negative levels reduce CPU
    /// usage at the cost of network traffic, like `-remoteWrite.vmProtoCompressLevel` in `vmagent`.
    /// If unset, the zstd default level (3) is used.
    #[configurable(metadata(docs::examples = 3))]
    #[configurable(metadata(docs::examples = -3))]
    pub compression_level: Option<i32>,

    /// The amount of time, in seconds, that incremental metrics persist in the internal metrics
    /// cache after having not been updated before they expire and are removed.
    ///
    /// If unset, sending unique incremental metrics to this sink causes indefinite memory growth.
    #[configurable(metadata(docs::examples = 300.0))]
    pub expire_metrics_secs: Option<f64>,

    #[serde(default)]
    pub batch: BatchConfig<VictoriaMetricsDefaultBatchSettings>,

    #[serde(default)]
    pub request: VictoriaMetricsRequestConfig,

    pub tls: Option<TlsConfig>,

    /// HTTP authentication.
    ///
    /// An event secret named `victoriametrics_token` overrides this setting for that event: the
    /// secret is sent as a bearer token. Use `set_secret` in VRL to choose a `vmauth` credential
    /// per event.
    pub auth: Option<Auth>,

    /// The retry strategy for failed requests.
    ///
    /// For this sink, `default` matches `vmagent`: requests rejected with 400, 409, or 415 are
    /// dropped, and every other failed request is retried, including 401 and 403, which `vmauth`
    /// returns while credentials are rotated.
    #[serde(default)]
    pub retry_strategy: RetryStrategy,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

/// How the VictoriaMetrics tenant is chosen.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct TenantConfig {
    #[serde(default)]
    pub mode: TenantMode,

    /// The tenant, as `accountID` or `accountID:projectID`.
    ///
    /// Required when `mode` is `path` or `labels`. Events whose rendered tenant is not a valid
    /// VictoriaMetrics tenant are rejected.
    #[configurable(metadata(docs::examples = "42"))]
    #[configurable(metadata(docs::examples = "42:{{ tags.project_id }}"))]
    pub id: Option<Template>,
}

/// Where the tenant is sent.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TenantMode {
    /// No tenant. Writes to `/api/v1/write`.
    ///
    /// Use this for single-node VictoriaMetrics, and for `vmauth`, which chooses the tenant from
    /// the credential.
    #[default]
    None,

    /// The tenant is part of the URL path. Writes to
    /// `/insert/<tenant>/prometheus/api/v1/write` on `vminsert`.
    Path,

    /// The tenant is sent in the `vm_account_id` and `vm_project_id` labels. Writes to
    /// `/insert/multitenant/prometheus/api/v1/write` on `vminsert`.
    Labels,
}

/// Outbound HTTP request settings.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default)]
pub struct VictoriaMetricsRequestConfig {
    #[serde(flatten)]
    pub tower: TowerRequestConfig,

    /// Additional HTTP headers to add to every HTTP request.
    ///
    /// Values are applied verbatim; template expansion is not supported.
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and its static value."
    ))]
    #[configurable(metadata(docs::examples = "request_headers_examples()"))]
    pub headers: BTreeMap<String, String>,
}

fn request_headers_examples() -> BTreeMap<String, String> {
    BTreeMap::from([("X-My-Custom-Header".to_string(), "A-Value".to_string())])
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VictoriaMetricsDefaultBatchSettings;

impl SinkBatchSettings for VictoriaMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(10_000);
    // Estimated encoded size, matching the `-remoteWrite.maxBlockSize` default of `vmagent`.
    const MAX_BYTES: Option<usize> = Some(8 * 1024 * 1024);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for VictoriaMetricsConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str("endpoint: http://localhost:8428").unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "victoriametrics")]
impl SinkConfig for VictoriaMetricsConfig {
    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }

    fn confinement_config(&self) -> Option<&ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::metric()
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedVictoriaMetrics {
    tenant_id: Option<ConfinedTemplate>,
    endpoint: WriteEndpoint,
    headers: Arc<BTreeMap<OrderedHeaderName, HeaderValue>>,
    batch_settings: BatcherSettings,
    expire_metrics: Option<Duration>,
    compression_level: i32,
    /// The write URL checked by the healthcheck, if it does not depend on event data.
    healthcheck_uri: Option<Uri>,
}

impl VictoriaMetricsConfig {
    fn validate_tenant(&self) -> crate::Result<Option<ConfinedTemplate>> {
        match (self.tenant.mode, &self.tenant.id) {
            (TenantMode::None, None) => Ok(None),
            (TenantMode::None, Some(_)) => {
                Err("`tenant.id` requires `tenant.mode` to be `path` or `labels`.".into())
            }
            (_, None) => {
                Err("`tenant.id` is required when `tenant.mode` is `path` or `labels`.".into())
            }
            (_, Some(template)) => {
                if !template.is_dynamic() && Tenant::parse(template.get_ref()).is_none() {
                    return Err(format!(
                        "`tenant.id` {:?} is not a valid VictoriaMetrics tenant. Use `accountID` or `accountID:projectID`.",
                        template.get_ref()
                    )
                    .into());
                }
                Ok(Some(template.clone().confine(
                    &self.confinement,
                    Self::NAME,
                    "tenant.id",
                )?))
            }
        }
    }

    fn validate_compression_level(&self) -> crate::Result<i32> {
        let level = self
            .compression_level
            .unwrap_or(zstd::DEFAULT_COMPRESSION_LEVEL);
        let range = zstd::compression_level_range();
        if range.contains(&level) {
            Ok(level)
        } else {
            Err(format!(
                "`compression_level` must be between {} and {}.",
                range.start(),
                range.end()
            )
            .into())
        }
    }

    fn validate_expire_metrics(&self) -> crate::Result<Option<Duration>> {
        self.expire_metrics_secs
            .map(|secs| {
                Duration::try_from_secs_f64(secs)
                    .ok()
                    .filter(|ttl| !ttl.is_zero())
                    .ok_or_else(|| "`expire_metrics_secs` must be a positive number.".into())
            })
            .transpose()
    }
}

fn validate_headers(
    headers: &BTreeMap<String, String>,
    configures_auth: bool,
) -> crate::Result<BTreeMap<OrderedHeaderName, HeaderValue>> {
    let headers = crate::sinks::util::http::validate_headers(headers)?;
    if configures_auth && headers.keys().any(|name| name.inner() == AUTHORIZATION) {
        return Err("Authorization header can not be used with defined auth options".into());
    }
    Ok(headers)
}

#[async_trait::async_trait]
impl ValidatedSink for VictoriaMetricsConfig {
    type Validated = ValidatedVictoriaMetrics;

    fn validate(&self) -> crate::Result<ValidatedVictoriaMetrics> {
        #[cfg(feature = "aws-core")]
        if matches!(self.auth, Some(Auth::Aws { .. })) {
            return Err(
                "The `aws` auth strategy is not supported by the `victoriametrics` sink.".into(),
            );
        }

        let tenant_id = self.validate_tenant()?;
        let expire_metrics = self.validate_expire_metrics()?;
        let compression_level = self.validate_compression_level()?;
        let headers = Arc::new(validate_headers(
            &self.request.headers,
            self.auth.is_some(),
        )?);
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let endpoint = match self.tenant.mode {
            TenantMode::None => {
                WriteEndpoint::Static(self.endpoint.append_path(WRITE_PATH)?.as_uri().clone())
            }
            TenantMode::Labels => WriteEndpoint::Static(
                self.endpoint
                    .append_path(MULTITENANT_WRITE_PATH)?
                    .as_uri()
                    .clone(),
            ),
            TenantMode::Path => WriteEndpoint::PerTenant(self.endpoint.clone()),
        };

        let healthcheck_uri = match &endpoint {
            WriteEndpoint::Static(uri) => Some(uri.clone()),
            WriteEndpoint::PerTenant(base) => self
                .tenant
                .id
                .as_ref()
                .filter(|template| !template.is_dynamic())
                .and_then(|template| Tenant::parse(template.get_ref()))
                .map(|tenant| base.append_path(&tenant_write_path(&tenant)))
                .transpose()?
                .map(|endpoint| endpoint.as_uri().clone()),
        };

        Ok(ValidatedVictoriaMetrics {
            tenant_id,
            endpoint,
            headers,
            batch_settings,
            expire_metrics,
            compression_level,
            healthcheck_uri,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedVictoriaMetrics,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let compression_level = validated.compression_level;
        let settings = RequestSettings {
            auth: self.auth.clone(),
            headers: Arc::clone(&validated.headers),
        };

        let healthcheck_uri = cx
            .healthcheck
            .uri
            .map(|uri| uri.uri)
            .or_else(|| validated.healthcheck_uri.clone());
        let healthcheck = match healthcheck_uri {
            Some(uri) => {
                healthcheck(client.clone(), uri, compression_level, settings.clone()).boxed()
            }
            // The write URL depends on event data, so there is nothing to check at startup.
            None => future::ok(()).boxed(),
        };

        let service = ServiceBuilder::new()
            .settings(
                self.request.tower.into_settings(),
                VmRetryLogic {
                    strategy: self.retry_strategy.clone(),
                },
            )
            .service(VmService {
                client,
                endpoint: validated.endpoint.clone(),
                settings,
            });

        let sink = VmSink {
            service,
            batch_settings: validated.batch_settings,
            request_builder: VmRequestBuilder {
                encoder: VmEncoder {
                    default_namespace: self.default_namespace.clone(),
                    send_metadata: self.send_metadata,
                    compression_level,
                },
            },
            tenant_mode: self.tenant.mode,
            tenant_id: validated.tenant_id.clone(),
            expire_metrics: validated.expire_metrics,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

/// Writes an empty request to `uri` with the configured credentials.
///
/// This checks reachability, authentication, and `vmauth` routing, unlike the `/health` endpoint.
async fn healthcheck(
    client: HttpClient,
    uri: Uri,
    compression_level: i32,
    settings: RequestSettings,
) -> crate::Result<()> {
    let body = compress(compression_level, &[])?.into();
    let request = settings.build_request(Method::POST, uri, body, None)?;
    let response = client.send(request).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(config: &str) -> VictoriaMetricsConfig {
        serde_yaml::from_str(config).unwrap()
    }

    fn validate_err(config: &str) -> String {
        parse(config).validate().unwrap_err().to_string()
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VictoriaMetricsConfig>();
    }

    #[test]
    fn write_url_depends_on_tenant_mode() {
        let uri = |config: &str| match parse(config).validate().unwrap().endpoint {
            WriteEndpoint::Static(uri) => uri.to_string(),
            WriteEndpoint::PerTenant(base) => format!("per-tenant {base}"),
        };

        assert_eq!(
            uri("endpoint: http://vm:8428"),
            "http://vm:8428/api/v1/write"
        );
        assert_eq!(
            uri("endpoint: http://vmauth:8427/prefix/"),
            "http://vmauth:8427/prefix/api/v1/write"
        );
        assert_eq!(
            uri("endpoint: http://vminsert:8480\ntenant: {mode: labels, id: '42'}"),
            "http://vminsert:8480/insert/multitenant/prometheus/api/v1/write"
        );
        assert_eq!(
            uri("endpoint: http://vminsert:8480\ntenant: {mode: path, id: '42'}"),
            "per-tenant http://vminsert:8480/"
        );
    }

    #[test]
    fn healthcheck_uri_for_static_tenant() {
        let validated = parse("endpoint: http://vminsert:8480\ntenant: {mode: path, id: '42:1'}")
            .validate()
            .unwrap();
        assert_eq!(
            validated.healthcheck_uri.unwrap().to_string(),
            "http://vminsert:8480/insert/42:1/prometheus/api/v1/write"
        );

        let validated =
            parse("endpoint: http://vminsert:8480\ntenant: {mode: path, id: '42:{{ tags.p }}'}")
                .validate()
                .unwrap();
        assert!(validated.healthcheck_uri.is_none());
    }

    #[test]
    fn rejects_invalid_tenant_config() {
        assert!(
            validate_err("endpoint: http://vm:8428\ntenant: {id: '42'}")
                .contains("requires `tenant.mode`")
        );
        assert!(
            validate_err("endpoint: http://vm:8428\ntenant: {mode: path}").contains("is required")
        );
        assert!(
            validate_err("endpoint: http://vm:8428\ntenant: {mode: path, id: 'abc'}")
                .contains("not a valid VictoriaMetrics tenant")
        );
    }

    #[test]
    fn confines_dynamic_tenant() {
        assert!(
            parse("endpoint: http://vm:8428\ntenant: {mode: path, id: '{{ tags.tenant }}'}")
                .validate()
                .is_err()
        );
        assert!(
            parse(
                "endpoint: http://vm:8428\ntenant: {mode: path, id: '{{ tags.tenant }}'}\ndangerously_allow_unconfined_template_resolution: true"
            )
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn rejects_authorization_header_with_auth() {
        let error = validate_err(
            "endpoint: http://vm:8428\nauth: {strategy: bearer, token: t}\nrequest: {headers: {Authorization: x}}",
        );
        assert!(error.contains("Authorization header"));
    }

    #[test]
    fn validates_compression_level() {
        let level = |config: &str| parse(config).validate().map(|v| v.compression_level);

        assert_eq!(level("endpoint: http://vm:8428").unwrap(), 3);
        assert_eq!(
            level("endpoint: http://vm:8428\ncompression_level: 19").unwrap(),
            19
        );
        assert_eq!(
            level("endpoint: http://vm:8428\ncompression_level: -5").unwrap(),
            -5
        );
        let error = validate_err("endpoint: http://vm:8428\ncompression_level: 23");
        assert!(error.contains("compression_level"));
    }

    #[test]
    fn rejects_invalid_expire_metrics() {
        for value in ["0", "-1"] {
            let error = validate_err(&format!(
                "endpoint: http://vm:8428\nexpire_metrics_secs: {value}"
            ));
            assert!(error.contains("expire_metrics_secs"), "{value}");
        }
    }

    #[cfg(feature = "aws-core")]
    #[test]
    fn rejects_aws_auth() {
        let error = validate_err("endpoint: http://vm:8428\nauth: {strategy: aws, service: aps}");
        assert!(error.contains("not supported"));
    }
}
