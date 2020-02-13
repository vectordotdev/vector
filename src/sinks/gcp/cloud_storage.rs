use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::{self, Event},
    sinks::{
        util::{
            http::{https_client, HttpsClient},
            retries::{RetryAction, RetryLogic},
            tls::{TlsOptions, TlsSettings},
            BatchBytesConfig, Buffer, PartitionBuffer, PartitionInnerBuffer, ServiceBuilderExt,
            SinkExt, TowerRequestConfig,
        },
        Healthcheck, RouterSink,
    },
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use chrono::Utc;
use futures::{stream::iter_ok, Future, Poll, Sink};
use http::{Method, StatusCode, Uri};
use hyper::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Body, Request,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::HashMap;
use tower::{Service, ServiceBuilder};
use tracing::field;
use uuid::Uuid;

const NAME: &str = "gcp_cloud_storage";
const BASE_URL: &str = "https://storage.googleapis.com/";

#[derive(Clone)]
struct GcsSink {
    client: HttpsClient,
    creds: Option<GcpCredentials>,
}

#[derive(Debug, Snafu)]
enum GcsHealthcheckError {
    #[snafu(display("Bucket {:?} not found", bucket))]
    BucketNotFound { bucket: String },
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct GcsSinkConfig {
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub filename_time_format: Option<String>,
    pub filename_append_uuid: Option<bool>,
    pub filename_extension: Option<String>,
    #[serde(flatten)]
    pub options: GcsOptions,
    encoding: Encoding,
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,
    pub tls: Option<TlsOptions>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GcsOptions {
    acl: Option<GcsPredefinedAcl>,
    storage_class: Option<GcsStorageClass>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "kebab-case")]
pub enum GcsPredefinedAcl {
    AuthenticatedRead,
    BucketOwnerFullControl,
    BucketOwnerRead,
    #[derivative(Default)]
    Private,
    PublicRead,
    ProjectPrivate,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum GcsServerSideEncryption {
    #[serde(rename = "AES256")]
    AES256,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GcsStorageClass {
    #[derivative(Default)]
    Standard,
    Nearline,
    Coldline,
    Archive,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        in_flight_limit: Some(25),
        rate_limit_num: Some(25),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Ndjson,
}

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Compression {
    #[derivative(Default)]
    Gzip,
    None,
}

inventory::submit! {
    SinkDescription::new::<GcsSinkConfig>(NAME)
}

#[typetag::serde(name = "gcp_cloud_storage")]
impl SinkConfig for GcsSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let creds = self.auth.make_credentials(Scope::DevStorageReadWrite)?;
        let healthcheck = GcsSink::healthcheck(self, &cx, &creds)?;
        let sink = GcsSink::new(self, cx, creds)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        NAME
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid credentials"))]
    InvalidCredentials,
    #[snafu(display("Unknown bucket: {:?}", bucket))]
    UnknownBucket { bucket: String },
    #[snafu(display("Unknown status code: {}", status))]
    UnknownStatus { status: http::StatusCode },
}

impl GcsSink {
    pub fn new(
        config: &GcsSinkConfig,
        cx: SinkContext,
        creds: Option<GcpCredentials>,
    ) -> crate::Result<RouterSink> {
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();

        let compression = match config.compression {
            Compression::Gzip => true,
            Compression::None => false,
        };
        let filename_time_format = config.filename_time_format.clone().unwrap_or("%s".into());
        let filename_append_uuid = config.filename_append_uuid.unwrap_or(true);
        let batch = config.batch.unwrap_or(bytesize::mib(10u64), 300);

        let key_prefix = if let Some(kp) = &config.key_prefix {
            Template::from(kp.as_str())
        } else {
            Template::from("date=%F/")
        };

        let tls = TlsSettings::from_options(&config.tls)?;
        let client = https_client(cx.resolver(), tls)?;

        let gcs = GcsSink { client, creds };

        let filename_extension = config.filename_extension.clone();
        let bucket = config.bucket.clone();
        let options = config.options.clone();

        let svc = ServiceBuilder::new()
            .map(move |req| {
                build_request(
                    req,
                    filename_time_format.clone(),
                    filename_extension.clone(),
                    filename_append_uuid,
                    compression,
                    bucket.clone(),
                    options.clone(),
                )
            })
            .settings(request, GcsRetryLogic)
            .service(gcs);

        let sink = crate::sinks::util::BatchServiceSink::new(svc, cx.acker())
            .partitioned_batched_with_min(PartitionBuffer::new(Buffer::new(compression)), &batch)
            .with_flat_map(move |e| iter_ok(encode_event(e, &key_prefix, &encoding)));

        Ok(Box::new(sink))
    }

    pub fn healthcheck(
        config: &GcsSinkConfig,
        cx: &SinkContext,
        creds: &Option<GcpCredentials>,
    ) -> crate::Result<Healthcheck> {
        let mut builder = Request::builder();
        builder.method(Method::HEAD);
        builder.uri(format!("{}{}/", BASE_URL, config.bucket).parse::<Uri>()?);

        let mut request = builder.body(Body::empty()).unwrap();
        if let Some(creds) = &creds {
            creds.apply(&mut request);
        }

        let tls = TlsSettings::from_options(&config.tls)?;
        let client = https_client(cx.resolver(), tls)?;

        let healthcheck =
            client
                .request(request)
                .map_err(Into::into)
                .and_then(healthcheck_response(
                    creds.clone(),
                    GcsHealthcheckError::BucketNotFound {
                        bucket: config.bucket.clone(),
                    }
                    .into(),
                ));

        Ok(Box::new(healthcheck))
    }
}

impl Service<RequestWrapper> for GcsSink {
    type Response = hyper::Response<Body>;
    type Error = hyper::Error;
    type Future = hyper::client::ResponseFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, request: RequestWrapper) -> Self::Future {
        let options = request.options;

        let uri = format!("{}{}/{}", BASE_URL, request.bucket, request.key)
            .parse::<Uri>()
            .unwrap();
        let mut builder = Request::builder();
        builder.method(Method::PUT);
        builder.uri(uri);
        let headers = builder.headers_mut().unwrap();
        add_header(headers, "content-type", "text/plain"); // FIXME needs proper content type
        add_header(headers, "content-length", format!("{}", request.body.len()));
        request
            .content_encoding
            .map(|ce| add_header(headers, "content-encoding", ce));
        options
            .acl
            .map(|acl| add_header(headers, "x-goog-acl", to_string(acl)));
        options
            .storage_class
            .map(|sc| add_header(headers, "x-goog-storage-class", to_string(sc)));
        if let Some(tags) = options.tags {
            for (p, v) in tags {
                headers.insert(
                    HeaderName::from_bytes(p.as_bytes()).unwrap(),
                    HeaderValue::from_str(&v).unwrap(),
                );
            }
        }

        let mut request = builder.body(Body::from(request.body)).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

        self.client.request(request)
    }
}

fn to_string(value: impl Serialize) -> String {
    serde_json::to_value(&value).unwrap().to_string()
}

fn add_header(headers: &mut HeaderMap, name: &'static str, value: impl AsRef<str>) {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value.as_ref()).unwrap(),
    );
}

fn build_request(
    req: PartitionInnerBuffer<Vec<u8>, Bytes>,
    time_format: String,
    extension: Option<String>,
    uuid: bool,
    gzip: bool,
    bucket: String,
    options: GcsOptions,
) -> RequestWrapper {
    let (body, key) = req.into_parts();

    // TODO: pull the seconds from the last event
    let filename = {
        let seconds = Utc::now().format(&time_format);

        if uuid {
            let uuid = Uuid::new_v4();
            format!("{}-{}", seconds, uuid.to_hyphenated())
        } else {
            seconds.to_string()
        }
    };

    let extension = extension.unwrap_or_else(|| if gzip { "log.gz" } else { "log" }.into());

    let key = String::from_utf8_lossy(&key[..]).into_owned();

    let key = format!("{}{}.{}", key, filename, extension);

    debug!(
        message = "sending events.",
        bytes = &field::debug(body.len()),
        bucket = &field::debug(&bucket),
        key = &field::debug(&key)
    );

    RequestWrapper {
        body,
        bucket,
        key,
        content_encoding: if gzip { Some("gzip".to_string()) } else { None },
        options,
    }
}

#[derive(Clone, Debug)]
struct RequestWrapper {
    body: Vec<u8>,
    bucket: String,
    key: String,
    content_encoding: Option<String>,
    options: GcsOptions,
}

fn encode_event(
    event: Event,
    key_prefix: &Template,
    encoding: &Encoding,
) -> Option<PartitionInnerBuffer<Vec<u8>, Bytes>> {
    let key = key_prefix
        .render_string(&event)
        .map_err(|missing_keys| {
            warn!(
                message = "Keys do not exist on the event. Dropping event.",
                ?missing_keys,
                rate_limit_secs = 30,
            );
        })
        .ok()?;

    let log = event.into_log();
    let bytes = match encoding {
        Encoding::Ndjson => serde_json::to_vec(&log.unflatten())
            .map(|mut b| {
                b.push(b'\n');
                b
            })
            .expect("Failed to encode event as json, this is a bug!"),
        &Encoding::Text => {
            let mut bytes = log
                .get(&event::MESSAGE)
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or_default();
            bytes.push(b'\n');
            bytes
        }
    };

    Some(PartitionInnerBuffer::new(bytes, key.into()))
}

#[derive(Clone)]
pub struct GcsRetryLogic;

// This is a clone of HttpRetryLogic for the Body type, should get merged
impl RetryLogic for GcsRetryLogic {
    type Error = hyper::Error;
    type Response = hyper::Response<Body>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_connect() || error.is_closed()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("Too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(format!("{}", status)),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};

    use std::collections::HashMap;

    #[test]
    fn gcs_encode_event_text() {
        let message = "hello world".to_string();
        let batch_time_format = Template::from("date=%F");
        let bytes =
            encode_event(message.clone().into(), &batch_time_format, &Encoding::Text).unwrap();

        let encoded_message = message + "\n";
        let (bytes, _) = bytes.into_parts();
        assert_eq!(&bytes[..], encoded_message.as_bytes());
    }

    #[test]
    fn gcs_encode_event_ndjson() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");

        let batch_time_format = Template::from("date=%F");
        let bytes = encode_event(event, &batch_time_format, &Encoding::Ndjson).unwrap();

        let (bytes, _) = bytes.into_parts();
        let map: HashMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(map[&event::MESSAGE.to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn gcs_build_request() {
        let buf = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("key/"));

        let req = build_request(
            buf.clone(),
            "date".into(),
            Some("ext".into()),
            false,
            false,
            "bucket".into(),
            GcsOptions::default(),
        );
        assert_eq!(req.key, "key/date.ext".to_string());

        let req = build_request(
            buf.clone(),
            "date".into(),
            None,
            false,
            false,
            "bucket".into(),
            GcsOptions::default(),
        );
        assert_eq!(req.key, "key/date.log".to_string());

        let req = build_request(
            buf.clone(),
            "date".into(),
            None,
            false,
            true,
            "bucket".into(),
            GcsOptions::default(),
        );
        assert_eq!(req.key, "key/date.log.gz".to_string());

        let req = build_request(
            buf.clone(),
            "date".into(),
            None,
            true,
            true,
            "bucket".into(),
            GcsOptions::default(),
        );
        assert_ne!(req.key, "key/date.log.gz".to_string());
    }
}
