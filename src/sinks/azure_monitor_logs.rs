use crate::{
    event::Event,
    sinks::{
        util::{
            encoding::{EncodingConfigWithDefault, EncodingConfiguration},
            http2::{BatchedHttpSink, HttpClient, HttpSink},
            service2::TowerRequestConfig,
            BatchBytesConfig, BoxedRawValue, JsonArrayBuffer,
        },
        Healthcheck, RouterSink,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{FutureExt, TryFutureExt};
use futures01::Sink;
use http02::{
    header, header::HeaderMap, header::HeaderName, header::HeaderValue, Request, StatusCode, Uri,
};
use hyper13::Body;
use lazy_static::lazy_static;
use openssl::{base64, hash, pkey, sign};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AzureMonitorLogsConfig {
    pub customer_id: String,
    pub shared_key: String,
    pub log_type: String,
    pub azure_resource_id: Option<String>,
    pub time_generated_field: Option<String>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        ..Default::default()
    };
    static ref LOG_TYPE_HEADER: HeaderName = HeaderName::from_static("log-type");
    static ref X_MS_DATE_HEADER: HeaderName = HeaderName::from_static(X_MS_DATE);
    static ref X_MS_AZURE_RESOURCE_HEADER: HeaderName =
        HeaderName::from_static("x-ms-azureresourceid");
    static ref TIME_GENERATED_FIELD_HEADER: HeaderName =
        HeaderName::from_static("time-generated-field");
    static ref CONTENT_TYPE_VALUE: HeaderValue = HeaderValue::from_static(CONTENT_TYPE);
}
inventory::submit! {
    SinkDescription::new::<AzureMonitorLogsConfig>("azure_monitor_logs")
}

/// Max number of bytes in request body
const MAX_BATCH_SIZE_MB: u64 = 30;
/// API endpoint for submitting logs
const RESOURCE: &'static str = "/api/logs";
/// JSON content type of logs
const CONTENT_TYPE: &'static str = "application/json";
/// Custom header used for signing logs
const X_MS_DATE: &'static str = "x-ms-date";
/// Shared key prefix
const SHARED_KEY: &'static str = "SharedKey";
/// API version
const API_VERSION: &'static str = "2016-04-01";

#[typetag::serde(name = "azure_monitor_logs")]
impl SinkConfig for AzureMonitorLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let batch = self.batch.unwrap_or(bytesize::mb(10u64), 1);
        if batch.size as u64 > bytesize::mb(MAX_BATCH_SIZE_MB) {
            warn!(
                "provided batch size is too big for Azure Monitor: {}",
                batch.size
            );
        }

        let url = format!(
            "https://{}.ods.opinsights.azure.com{}?api-version={}",
            self.customer_id, RESOURCE, API_VERSION
        );
        let uri: Uri = url.parse()?;

        let shared_key_bytes = base64::decode_block(&self.shared_key)?;
        let shared_key = pkey::PKey::hmac(&shared_key_bytes)?;
        let mut default_headers = HeaderMap::with_capacity(5);

        let log_type = HeaderValue::from_str(&self.log_type)?;
        default_headers.insert(LOG_TYPE_HEADER.clone(), log_type);

        if let Some(time_generated_field) = &self.time_generated_field {
            if !time_generated_field.is_empty() {
                default_headers.insert(
                    TIME_GENERATED_FIELD_HEADER.clone(),
                    HeaderValue::from_str(time_generated_field)?,
                );
            }
        } else {
            default_headers.insert(
                TIME_GENERATED_FIELD_HEADER.clone(),
                HeaderValue::from_static("timestamp"),
            );
        }

        if let Some(azure_resource_id) = &self.azure_resource_id {
            default_headers.insert(
                TIME_GENERATED_FIELD_HEADER.clone(),
                HeaderValue::from_str(azure_resource_id)?,
            );
        }

        default_headers.insert(header::CONTENT_TYPE, CONTENT_TYPE_VALUE.clone());

        let sink = AzureMonitorLogsSink {
            uri,
            encoding: self.encoding.clone(),
            customer_id: self.customer_id.clone(),
            shared_key,
            default_headers,
        };
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let healthcheck = healthcheck(
            cx.clone(),
            sink.clone(),
            TlsSettings::from_options(&self.tls)?,
        )
        .boxed()
        .compat();

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::default(),
            request,
            batch,
            tls_settings,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal azure_monitor_logs sink error: {}", e));

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "azure_monitor_logs"
    }
}

#[derive(Clone)]
struct AzureMonitorLogsSink {
    uri: Uri,
    customer_id: String,
    encoding: EncodingConfigWithDefault<Encoding>,
    shared_key: pkey::PKey<pkey::Private>,
    default_headers: HeaderMap,
}

impl HttpSink for AzureMonitorLogsSink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);

        let entry = serde_json::json!(event.into_log());

        Some(entry)
    }

    fn build_request(&self, events: Self::Output) -> Request<Vec<u8>> {
        let events = serde_json::json!([events]);

        let body = serde_json::to_vec(&events).unwrap();
        let len = body.len();

        let mut request = Request::post(self.uri.clone()).body(body).unwrap();

        let rfc1123date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();

        let authorization = self.build_authorization_header_value(&rfc1123date, len).unwrap();
        *request.headers_mut() = self.default_headers.clone();
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, authorization.parse().unwrap());
        request
            .headers_mut()
            .insert(X_MS_DATE_HEADER.clone(), rfc1123date.parse().unwrap());

        request
    }
}

impl AzureMonitorLogsSink {
    fn build_authorization_header_value(&self, rfc1123date: &str, len: usize) -> crate::Result<String> {
        let string_to_hash = format!(
            "POST\n{}\n{}\n{}:{}\n{}",
            len, CONTENT_TYPE, X_MS_DATE, rfc1123date, RESOURCE
        );
        let signer = sign::Signer::new(hash::MessageDigest::sha256(), &self.shared_key)?;

        // needs mut signer starting from openssl 0.10.28
        let signature = signer.sign_oneshot_to_vec(string_to_hash.as_bytes())?;
        let signature_base64 = base64::encode_block(&signature);

        Ok(format!(
            "{} {}:{}",
            SHARED_KEY, self.customer_id, signature_base64
        ))
    }
}

async fn healthcheck(
    cx: SinkContext,
    sink: AzureMonitorLogsSink,
    tls: TlsSettings,
) -> crate::Result<()> {
    let request = sink.build_request(vec![]).map(Body::from);

    let mut client = HttpClient::new(cx.resolver(), tls)?;
    let res = client.send(request).await?;

    if res.status().is_server_error() {
        return Err(format!("Server returned a server error.").into());
    }

    if res.status() == StatusCode::FORBIDDEN {
        return Err(format!("The service failed to authenticate the request. Verify that the workspace ID and connection key are valid.").into());
    }

    if res.status() == StatusCode::NOT_FOUND {
        return Err(format!("Either the URL provided is incorrect, or the request is too large.").into());
    }

    if res.status() == StatusCode::BAD_REQUEST {
        return Err(format!("The workspace has been closed or the request was invalid.").into());
    }

    Ok(())
}
