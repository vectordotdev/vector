use lapin::tcp::{OwnedIdentity, OwnedTLSConfig};
use vector_config::configurable_component;

/// Connection options for `AMQP`.
#[configurable_component]
#[derive(Clone, Debug)]
pub(crate) struct AmqpConfig {
    /// URI for the `AMQP` server.
    ///
    /// Format: amqp://<user>:<password>@<host>:<port>/<vhost>?timeout=<seconds>
    pub(crate) connection_string: String,

    #[configurable(derived)]
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
    pub(crate) async fn connect(
        &self,
    ) -> Result<(lapin::Connection, lapin::Channel), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Connecting to {}.", self.connection_string);
        let addr = self.connection_string.clone();
        let conn = match &self.tls {
            Some(tls) => {
                let cert_chain = if let Some(ca) = &tls.ca_file {
                    Some(tokio::fs::read_to_string(ca.to_owned()).await?)
                } else {
                    None
                };
                let identity = if let Some(identity) = &tls.key_file {
                    let der = tokio::fs::read(identity.to_owned()).await?;
                    Some(OwnedIdentity {
                        der,
                        password: tls
                            .key_pass
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(String::default),
                    })
                } else {
                    None
                };
                let tls_config = OwnedTLSConfig {
                    identity,
                    cert_chain,
                };
                lapin::Connection::connect_with_config(
                    &addr,
                    lapin::ConnectionProperties::default(),
                    tls_config,
                )
                .await
            }
            None => lapin::Connection::connect(&addr, lapin::ConnectionProperties::default()).await,
        }?;
        let channel = conn.create_channel().await?;
        Ok((conn, channel))
    }
}
