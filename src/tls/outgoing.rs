use super::{tls_connector, Connect, Handshake, MaybeTlsSettings, MaybeTlsStream};
use snafu::ResultExt;
use std::net::SocketAddr;
use tokio::net::TcpStream;

impl MaybeTlsSettings {
    pub(crate) async fn connect(
        &self,
        host: &str,
        addr: &SocketAddr,
    ) -> crate::tls::Result<MaybeTlsStream<TcpStream>> {
        let stream = TcpStream::connect(addr).await.context(Connect)?;

        match self {
            MaybeTlsSettings::Raw(()) => Ok(MaybeTlsStream::Raw(stream)),
            MaybeTlsSettings::Tls(_) => {
                let config = tls_connector(self)?;
                let stream = tokio_openssl::connect(config, host, stream)
                    .await
                    .context(Handshake)?;

                debug!(message = "Negotiated TLS.");

                Ok(MaybeTlsStream::Tls(stream))
            }
        }
    }
}
