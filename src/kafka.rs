use std::path::{Path, PathBuf};

use rdkafka::{consumer::ConsumerContext, ClientConfig, ClientContext, Statistics};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use crate::{internal_events::KafkaStatisticsReceived, tls::TlsOptions};

#[derive(Debug, Snafu)]
enum KafkaError {
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum KafkaCompression {
    #[derivative(Default)]
    None,
    Gzip,
    Snappy,
    Lz4,
    Zstd,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaAuthConfig {
    pub sasl: Option<KafkaSaslConfig>,
    pub tls: Option<KafkaTlsConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaSaslConfig {
    pub enabled: Option<bool>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub mechanism: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KafkaTlsConfig {
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub options: TlsOptions,
}

impl KafkaAuthConfig {
    pub fn apply(&self, client: &mut ClientConfig) -> crate::Result<()> {
        let sasl_enabled = self.sasl.as_ref().and_then(|s| s.enabled).unwrap_or(false);
        let tls_enabled = self.tls.as_ref().and_then(|s| s.enabled).unwrap_or(false);

        let protocol = match (sasl_enabled, tls_enabled) {
            (false, false) => "plaintext",
            (false, true) => "ssl",
            (true, false) => "sasl_plaintext",
            (true, true) => "sasl_ssl",
        };
        client.set("security.protocol", protocol);

        if sasl_enabled {
            let sasl = self.sasl.as_ref().unwrap();
            if let Some(username) = &sasl.username {
                client.set("sasl.username", username);
            }
            if let Some(password) = &sasl.password {
                client.set("sasl.password", password);
            }
            if let Some(mechanism) = &sasl.mechanism {
                client.set("sasl.mechanism", mechanism);
            }
        }

        if tls_enabled {
            let tls = self.tls.as_ref().unwrap();
            if let Some(path) = &tls.options.ca_file {
                client.set("ssl.ca.location", pathbuf_to_string(path)?);
            }
            if let Some(path) = &tls.options.crt_file {
                client.set("ssl.certificate.location", pathbuf_to_string(path)?);
            }
            if let Some(path) = &tls.options.key_file {
                client.set("ssl.key.location", pathbuf_to_string(path)?);
            }
            if let Some(pass) = &tls.options.key_pass {
                client.set("ssl.key.password", pass);
            }
        }

        Ok(())
    }
}

fn pathbuf_to_string(path: &Path) -> crate::Result<&str> {
    path.to_str()
        .ok_or_else(|| KafkaError::InvalidPath { path: path.into() }.into())
}

pub struct KafkaStatisticsContext;

impl ClientContext for KafkaStatisticsContext {
    fn stats(&self, statistics: Statistics) {
        emit!(&KafkaStatisticsReceived {
            statistics: &statistics
        });
    }
}

impl ConsumerContext for KafkaStatisticsContext {}
