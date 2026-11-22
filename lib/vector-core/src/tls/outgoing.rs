use std::{net::SocketAddr, pin::Pin};

use snafu::ResultExt;
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

use super::{
    ConnectSnafu, HandshakeSnafu, MaybeTlsSettings, MaybeTlsStream, SslBuildSnafu, tls_connector,
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
                let config = tls_connector(self, host)?;
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use openssl::ssl::{NameType, SslAcceptor, SslMethod};
    use tokio::net::TcpListener;

    use super::*;
    use crate::tls::{TlsConfig, TlsSettings, test_util::self_signed_cert_for_ip};

    #[tokio::test]
    async fn bracketed_ipv6_host_sends_no_sni_and_verifies_the_ip() {
        let (cert, key) = self_signed_cert_for_ip("::1");
        let mut acceptor = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls()).unwrap();
        acceptor.set_certificate(&cert).unwrap();
        acceptor.set_private_key(&key).unwrap();
        let received_sni = Arc::new(Mutex::new(None));
        let sni = Arc::clone(&received_sni);
        acceptor.set_servername_callback(move |ssl, _| {
            *sni.lock().unwrap() = ssl.servername(NameType::HOST_NAME).map(str::to_owned);
            Ok(())
        });
        let acceptor = acceptor.build();
        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ssl = openssl::ssl::Ssl::new(acceptor.context()).unwrap();
            let mut stream = SslStream::new(ssl, stream).unwrap();
            Pin::new(&mut stream).accept().await
        });
        let ca_pem = String::from_utf8(cert.to_pem().unwrap()).unwrap();
        let config = TlsConfig {
            ca_file: Some(ca_pem.into()),
            ..Default::default()
        };
        let settings = MaybeTlsSettings::Tls(TlsSettings::from_options(Some(&config)).unwrap());

        let client = settings.connect("[::1]", &addr).await;

        assert!(client.is_ok(), "handshake failed: {:?}", client.err());
        assert!(server.await.unwrap().is_ok());
        assert_eq!(*received_sni.lock().unwrap(), None);
    }
}
