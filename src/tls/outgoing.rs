use super::{tls_connector, Connect, Handshake, MaybeTlsSettings, MaybeTlsStream, SslBuildError};
use snafu::ResultExt;
use std::{net::SocketAddr, pin::Pin};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

impl MaybeTlsSettings {
    // TODO: Fix interdependencies / component features that make this necessary.
    #[allow(dead_code)]
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
                let ssl = config.into_ssl(host).context(SslBuildError)?;

                let mut stream = SslStream::new(ssl, stream).context(SslBuildError)?;
                Pin::new(&mut stream).connect().await.context(Handshake)?;

                debug!(message = "Negotiated TLS.");

                Ok(MaybeTlsStream::Tls(stream))
            }
        }
    }
}
