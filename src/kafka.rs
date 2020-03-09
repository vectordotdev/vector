use crate::tls::TlsOptions;
use rdkafka::ClientConfig;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::path::PathBuf;

#[derive(Debug, Snafu)]
enum KafkaError {
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum KafkaCompression {
    #[derivative(Default)]
    None,
    Gzip,
    Snappy,
    Lz4,
    Zstd,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct KafkaTlsConfig {
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub options: TlsOptions,
}

impl KafkaTlsConfig {
    pub(crate) fn apply(&self, client: &mut ClientConfig) -> crate::Result<()> {
        client.set(
            "security.protocol",
            if self.enabled() { "ssl" } else { "plaintext" },
        );
        if let Some(ref path) = self.options.ca_path {
            client.set("ssl.ca.location", pathbuf_to_string(&path)?);
        }
        if let Some(ref path) = self.options.crt_path {
            client.set("ssl.certificate.location", pathbuf_to_string(&path)?);
        }
        if let Some(ref path) = self.options.key_path {
            client.set("ssl.keystore.location", pathbuf_to_string(&path)?);
        }
        if let Some(ref pass) = self.options.key_pass {
            client.set("ssl.keystore.password", pass);
        }
        Ok(())
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

fn pathbuf_to_string(path: &PathBuf) -> crate::Result<&str> {
    path.to_str()
        .ok_or_else(|| KafkaError::InvalidPath { path: path.into() }.into())
}
