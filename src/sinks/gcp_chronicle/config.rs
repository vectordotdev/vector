use vector_lib::config::AcknowledgementsConfig;

use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfig,
    gcp::GcpAuthConfig,
    sinks::{
        gcp_chronicle::{
            compression::ChronicleCompression,
            ChronicleConfigError, ChronicleDefaultBatchSettings,
            ChronicleTowerRequestConfigDefaults,
        },
        util::{BatchConfig, TowerRequestConfig},
    },
    tls::TlsConfig,
};

/// Google Chronicle regions.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// European Multi region - "https://europe-malachiteingestion-pa.googleapis.com".
    Eu,

    /// US Multi region - "https://malachiteingestion-pa.googleapis.com".
    Us,

    /// APAC region (this is the same as the Singapore region endpoint retained for backwards compatibility) - "https://asia-southeast1-malachiteingestion-pa.googleapis.com".
    Asia,

    /// SãoPaulo Region - "https://southamerica-east1-malachiteingestion-pa.googleapis.com"
    SãoPaulo,

    /// Canada Region - "https://northamerica-northeast2-malachiteingestion-pa.googleapis.com"
    Canada,

    /// Dammam Region - "https://me-central2-malachiteingestion-pa.googleapis.com"
    Dammam,

    /// Doha Region - "https://me-central1-malachiteingestion-pa.googleapis.com"
    Doha,

    /// Frankfurt Region - "https://europe-west3-malachiteingestion-pa.googleapis.com"
    Frankfurt,

    /// London Region - "https://europe-west2-malachiteingestion-pa.googleapis.com"
    London,

    /// Mumbai Region - "https://asia-south1-malachiteingestion-pa.googleapis.com"
    Mumbai,

    /// Paris Region - "https://europe-west9-malachiteingestion-pa.googleapis.com"
    Paris,

    /// Singapore Region - "https://asia-southeast1-malachiteingestion-pa.googleapis.com"
    Singapore,

    /// Sydney Region - "https://australia-southeast1-malachiteingestion-pa.googleapis.com"
    Sydney,

    /// TelAviv Region - "https://me-west1-malachiteingestion-pa.googleapis.com"
    TelAviv,

    /// Tokyo Region - "https://asia-northeast1-malachiteingestion-pa.googleapis.com"
    Tokyo,

    /// Turin Region - "https://europe-west12-malachiteingestion-pa.googleapis.com"
    Turin,

    /// Zurich Region - "https://europe-west6-malachiteingestion-pa.googleapis.com"
    Zurich,
}

impl Region {
    /// Each region has a its own endpoint.
    const fn endpoint(self) -> &'static str {
        match self {
            Region::Eu => "https://europe-malachiteingestion-pa.googleapis.com",
            Region::Us => "https://malachiteingestion-pa.googleapis.com",
            Region::Asia => "https://asia-southeast1-malachiteingestion-pa.googleapis.com",
            Region::SãoPaulo => "https://southamerica-east1-malachiteingestion-pa.googleapis.com",
            Region::Canada => {
                "https://northamerica-northeast2-malachiteingestion-pa.googleapis.com"
            }
            Region::Dammam => "https://me-central2-malachiteingestion-pa.googleapis.com",
            Region::Doha => "https://me-central1-malachiteingestion-pa.googleapis.com",
            Region::Frankfurt => "https://europe-west3-malachiteingestion-pa.googleapis.com",
            Region::London => "https://europe-west2-malachiteingestion-pa.googleapis.com",
            Region::Mumbai => "https://asia-south1-malachiteingestion-pa.googleapis.com",
            Region::Paris => "https://europe-west9-malachiteingestion-pa.googleapis.com",
            Region::Singapore => "https://asia-southeast1-malachiteingestion-pa.googleapis.com",
            Region::Sydney => "https://australia-southeast1-malachiteingestion-pa.googleapis.com",
            Region::TelAviv => "https://me-west1-malachiteingestion-pa.googleapis.com",
            Region::Tokyo => "https://asia-northeast1-malachiteingestion-pa.googleapis.com",
            Region::Turin => "https://europe-west12-malachiteingestion-pa.googleapis.com",
            Region::Zurich => "https://europe-west6-malachiteingestion-pa.googleapis.com",
        }
    }
}


/// Shared configuration for all GCP Chronicle sinks
/// Contains the maximum set of common settings that applies to all GCP Chronicle sink components.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ChronicleCommonConfig {
    /// The endpoint to send data to.
    #[configurable(metadata(
        docs::examples = "127.0.0.1:8080",
        docs::examples = "example.com:12345"
    ))]
    pub endpoint: Option<String>,

    /// The GCP region to use.
    #[configurable(derived)]
    pub region: Option<Region>,

    /// The Unique identifier (UUID) corresponding to the Chronicle instance.
    #[configurable(validation(format = "uuid"))]
    #[configurable(metadata(docs::examples = "c8c65bfa-5f2c-42d4-9189-64bb7b939f2c"))]
    pub customer_id: String,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<ChronicleDefaultBatchSettings>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[serde(default)]
    #[configurable(derived)]
    pub compression: ChronicleCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig<ChronicleTowerRequestConfigDefaults>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl ChronicleCommonConfig {
    pub fn create_endpoint(&self, path: &str) -> Result<String, ChronicleConfigError> {
        Ok(format!(
            "{}/{}",
            match (&self.endpoint, self.region) {
                (Some(endpoint), None) => endpoint.trim_end_matches('/'),
                (None, Some(region)) => region.endpoint(),
                (Some(_), Some(_)) => return Err(ChronicleConfigError::BothRegionAndEndpoint),
                (None, None) => return Err(ChronicleConfigError::RegionOrEndpoint),
            },
            path
        ))
    }
}
