use crate::region;
use futures::{Future, Sink};
use snafu::Snafu;
use std::path::PathBuf;

pub mod aws_cloudwatch_logs;
pub mod aws_cloudwatch_metrics;
pub mod aws_kinesis_streams;
pub mod aws_s3;
pub mod blackhole;
pub mod clickhouse;
pub mod console;
pub mod elasticsearch;
pub mod http;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod prometheus;
pub mod splunk_hec;
pub mod tcp;
pub mod util;
pub mod vector;

use crate::Event;

pub type RouterSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

pub type Healthcheck = Box<dyn Future<Item = (), Error = String> + Send>;

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Socket address problem: {}", source))]
    SocketAddressError { source: std::io::Error },
    #[snafu(display("{}", source))]
    RegionParseError { source: region::ParseError },
    #[snafu(display("Unable to resolve DNS for provided address"))]
    DNSFailure,
    #[snafu(display("Could not open {} file {:?}: {}", note, filename, source))]
    FileOpenFailed {
        note: &'static str,
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read {} file {:?}: {}", note, filename, source))]
    FileReadFailed {
        note: &'static str,
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Must specify both TLS key_file and crt_file"))]
    MissingCrtKeyFile,
    #[snafu(display("Could not build TLS connector: {}", source))]
    TlsBuildError { source: native_tls::Error },
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: native_tls::Error },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: openssl::error::ErrorStack },
    #[snafu(display("Could not parse certificate in {:?}: {}", filename, source))]
    CertificateParseError {
        filename: PathBuf,
        source: native_tls::Error,
    },
    #[snafu(display("Could not parse X509 certificate in {:?}: {}", filename, source))]
    X509ParseError {
        filename: PathBuf,
        source: openssl::error::ErrorStack,
    },
    #[snafu(display("Could not parse private key in {:?}: {}", filename, source))]
    PrivateKeyParseError {
        filename: PathBuf,
        source: openssl::error::ErrorStack,
    },
    #[snafu(display("Could not build PKCS#12 archive for identity: {}", source))]
    Pkcs12Error { source: openssl::error::ErrorStack },
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
    #[snafu(display("Host must include a scheme (https or http)"))]
    UriMissingScheme,
    #[snafu(display("{}: {}", source, name))]
    InvalidHeaderName {
        name: String,
        source: ::http::header::InvalidHeaderName,
    },
    #[snafu(display("{}: {}", source, value))]
    InvalidHeaderValue {
        value: String,
        source: ::http::header::InvalidHeaderValue,
    },
    #[snafu(display("Flush period for sets must be greater or equal to {}ms", min))]
    FlushPeriodTooShort { min: u64 },
    #[cfg(feature = "rdkafka")]
    #[snafu(display("Error creating kafka producer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_core::CredentialsError,
    },
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
}
