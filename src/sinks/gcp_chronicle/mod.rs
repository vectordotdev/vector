use bytes::Bytes;
use vector_lib::event::{EventFinalizers, Finalizable};
use vector_lib::request_metadata::{MetaDescriptive, RequestMetadata};

use std::collections::HashMap;
use http::{header::{HeaderValue, HeaderName}, StatusCode};
use snafu::Snafu;

use crate::{
    template::TemplateParseError,
    sinks::util::{service::TowerRequestConfigDefaults, SinkBatchSettings}
};

pub mod compression;
pub mod service;
pub mod config;

#[cfg(feature = "sinks-gcp-chronicle-udm-events")]
pub mod udm_events;

#[cfg(feature = "sinks-gcp-chronicle-unstructured")]
pub mod unstructured_logs;

#[derive(Clone, Copy, Debug)]
pub struct ChronicleTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for ChronicleTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 1_000;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ChronicleDefaultBatchSettings;
impl SinkBatchSettings for ChronicleDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 15.0;
}

#[derive(Clone, Debug)]
pub struct ChronicleRequest {
    pub headers: HashMap<HeaderName, HeaderValue>,
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for ChronicleRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for ChronicleRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

struct ChronicleRequestPayload {
    bytes: Bytes,
}

impl From<Bytes> for ChronicleRequestPayload {
    fn from(bytes: Bytes) -> Self {
        Self { bytes }
    }
}

impl AsRef<[u8]> for ChronicleRequestPayload {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

#[derive(Debug, Snafu)]
pub enum ChronicleConfigError {
    #[snafu(display("Region or endpoint not defined"))]
    RegionOrEndpoint,
    #[snafu(display("You can only specify one of region or endpoint"))]
    BothRegionAndEndpoint,
}

#[derive(Debug, Snafu)]
pub enum ChronicleResponseError {
    #[snafu(display("Server responded with an error: {} - {}", code, message))]
    ServerError { code: StatusCode, message: String },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("log_type template parse error: {}", source))]
    LogTypeTemplate { source: TemplateParseError },

    #[snafu(display("Endpoint not found"))]
    NotFound,
}
