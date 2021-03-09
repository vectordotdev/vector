use lapin::tcp::{OwnedIdentity, OwnedTLSConfig};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_amqp::*;

/// Client certificate for rabbit authentication
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClientCertDer {
    /// Certificate embedded in config file as base64 string
    Embedded(String),
    /// Certificate on file system
    Path(PathBuf),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AmqpConfig {
    /// Format: amqp://user:password@host:port/vhost?timeout=seconds
    pub(crate) connection_string: String,
    pub(crate) tls: Option<crate::tls::TlsConfig>,
}

impl Default for AmqpConfig {
    fn default() -> Self {
        Self {
            connection_string: "amqp://127.0.0.1/%2f".to_string(),
            tls: None,
        }
    }
}

impl AmqpConfig {
    pub async fn connect(
        &self,
    ) -> Result<(lapin::Connection, lapin::Channel), Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to {}", self.connection_string);
        let addr = self.connection_string.clone();
        let conn = match &self.tls {
            Some(tls) => {
                let cert_chain = if let Some(ca) = &tls.options.ca_file {
                    Some(tokio::fs::read_to_string(ca.to_owned()).await?)
                } else {
                    None
                };
                let identity = if let Some(identity) = &tls.options.key_file {
                    let der = tokio::fs::read(identity.to_owned()).await?;
                    Some(OwnedIdentity {
                        der: der,
                        password: tls
                            .options
                            .key_pass
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| String::default()),
                    })
                } else {
                    None
                };
                let tls_config = OwnedTLSConfig {
                    identity: identity,
                    cert_chain: cert_chain,
                };
                lapin::Connection::connect_with_config(
                    &addr,
                    lapin::ConnectionProperties::default().with_tokio(),
                    tls_config.as_ref(),
                )
                .await
            }
            None => {
                lapin::Connection::connect(
                    &addr,
                    lapin::ConnectionProperties::default().with_tokio(),
                )
                .await
            }
        }?;
        let channel = conn.create_channel().await?;
        Ok((conn, channel))
    }
}
