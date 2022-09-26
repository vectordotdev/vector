use std::{net::SocketAddr, pin::Pin};

use snafu::ResultExt;
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

use super::{
    tls_connector, ConnectSnafu, HandshakeSnafu, MaybeTlsSettings, MaybeTlsStream, SslBuildSnafu,
};

impl MaybeTlsSettings {
    pub async fn connect(
        &self,
        host: &str,
        addr: &SocketAddr,
    ) -> crate::tls::Result<MaybeTlsStream<TcpStream>> {
        let stream = TcpStream::connect(addr).await.context(ConnectSnafu)?;

        match self {
            MaybeTlsSettings::Raw(()) => Ok(MaybeTlsStream::Raw(stream)),
            MaybeTlsSettings::Tls(_) => {
                let config = tls_connector(self)?;
                let ssl = config.into_ssl(host).context(SslBuildSnafu)?;

                let mut stream = SslStream::new(ssl, stream).context(SslBuildSnafu)?;
                Pin::new(&mut stream)
                    .connect()
                    .await
                    .context(HandshakeSnafu)?;

                debug!(message = "Negotiated TLS.");

                Ok(MaybeTlsStream::Tls(stream))
            }
        }
    }
}
