use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use arc_swap::ArcSwap;
use futures::{FutureExt, Stream, future::BoxFuture, stream};
use ipnet::IpNet;
use openssl::{
    ssl::{Ssl, SslAcceptor, SslMethod},
    x509::X509,
};
use snafu::ResultExt;
use tokio::{
    io::{self, AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tokio_openssl::SslStream;
use tonic::transport::{Certificate, server::Connected};

use super::{
    CreateAcceptorSnafu, HandshakeSnafu, IncomingListenerSnafu, MaybeTlsSettings, MaybeTlsStream,
    SslBuildSnafu, TcpBindSnafu, TlsAcceptorReloader, TlsError, TlsSettings,
};
use crate::tcp::{self, TcpKeepaliveConfig};

impl TlsSettings {
    pub fn acceptor(&self) -> crate::tls::Result<SslAcceptor> {
        if self.identity.is_none() {
            Err(TlsError::MissingRequiredIdentity)
        } else {
            let mut acceptor =
                SslAcceptor::mozilla_intermediate(SslMethod::tls()).context(CreateAcceptorSnafu)?;
            self.apply_context_base(&mut acceptor, true)?;
            // When the server verifies client certificates, OpenSSL refuses to resume a session
            // unless a session ID context is set, and aborts the handshake with a fatal
            // `internal_error` alert instead. Clients which cache sessions would then fail every
            // reconnect once. A fixed value suffices because each acceptor has its own session
            // cache and ticket keys, so sessions cannot be resumed across acceptors.
            acceptor
                .set_session_id_context(b"vector")
                .context(CreateAcceptorSnafu)?;
            Ok(acceptor.build())
        }
    }
}

impl MaybeTlsSettings {
    pub async fn bind(&self, addr: &SocketAddr) -> crate::tls::Result<MaybeTlsListener> {
        self.bind_reloadable(addr, None).await
    }

    pub async fn bind_with_allowlist(
        &self,
        addr: &SocketAddr,
        allow_origin: Vec<IpNet>,
    ) -> crate::tls::Result<MaybeTlsListener> {
        Ok(self
            .bind_reloadable(addr, None)
            .await?
            .with_allowlist(Some(allow_origin)))
    }

    /// Bind a listener, optionally sharing a [`TlsAcceptorReloader`] so its acceptor can be swapped
    /// at runtime. When `reloader` is `Some` and TLS is enabled, connections accepted by the
    /// returned listener use the acceptor the reloader currently holds; otherwise the acceptor is
    /// fixed for the lifetime of the listener.
    pub async fn bind_reloadable(
        &self,
        addr: &SocketAddr,
        reloader: Option<TlsAcceptorReloader>,
    ) -> crate::tls::Result<MaybeTlsListener> {
        let listener = TcpListener::bind(addr).await.context(TcpBindSnafu)?;

        let acceptor = match (self, reloader) {
            (Self::Raw(()), _) => None,
            (Self::Tls(_), Some(reloader)) => Some(reloader.shared()),
            (Self::Tls(tls), None) => Some(Arc::new(ArcSwap::from_pointee(tls.acceptor()?))),
        };

        Ok(MaybeTlsListener {
            listener,
            acceptor,
            origin_filter: None,
            keepalive: None,
        })
    }
}

pub struct MaybeTlsListener {
    listener: TcpListener,
    acceptor: Option<Arc<ArcSwap<SslAcceptor>>>,
    origin_filter: Option<Vec<IpNet>>,
    keepalive: Option<TcpKeepaliveConfig>,
}

impl MaybeTlsListener {
    pub async fn accept(&mut self) -> crate::tls::Result<MaybeTlsIncomingStream<TcpStream>> {
        let (stream, peer_addr) = self
            .listener
            .accept()
            .await
            .context(IncomingListenerSnafu)?;

        if let Some(keepalive) = self.keepalive
            && let Some(time_secs) = keepalive.time_secs
        {
            let config =
                socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(time_secs));
            if let Err(error) = tcp::set_keepalive(&stream, &config) {
                warn!(message = "Failed to set TCP keepalive on accepted connection.", %error);
            }
        }

        let listener = MaybeTlsIncomingStream::new(
            stream,
            peer_addr,
            self.acceptor
                .as_ref()
                .map(|accptr| accptr.load().as_ref().clone()),
        );

        if let Some(origin_filter) = &self.origin_filter {
            if origin_filter
                .iter()
                .any(|net| net.contains(&listener.peer_addr().ip()))
            {
                Ok(listener)
            } else {
                Err(TlsError::Connect {
                    source: std::io::ErrorKind::ConnectionRefused.into(),
                })
            }
        } else {
            Ok(listener)
        }
    }

    async fn into_accept(
        mut self,
    ) -> (crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>, Self) {
        (self.accept().await, self)
    }

    pub fn accept_stream(
        self,
    ) -> impl Stream<Item = crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>> {
        let mut accept = Box::pin(self.into_accept());
        stream::poll_fn(move |context| match accept.as_mut().poll(context) {
            Poll::Ready((item, this)) => {
                accept.set(this.into_accept());
                Poll::Ready(Some(item))
            }
            Poll::Pending => Poll::Pending,
        })
    }

    pub fn accept_stream_limited(
        self,
        max_connections: Option<u32>,
    ) -> impl Stream<
        Item = (
            crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>,
            Option<OwnedSemaphorePermit>,
        ),
    > {
        let mut connection_semaphore_future = max_connections.map(|max| {
            let semaphore = Arc::new(Semaphore::new(max as usize));
            let future = Box::pin(semaphore.clone().acquire_owned());
            (semaphore, future)
        });

        let mut accept = Box::pin(self.into_accept());
        stream::poll_fn(move |context| {
            let permit = match connection_semaphore_future.as_mut() {
                Some((semaphore, future)) => match future.as_mut().poll(context) {
                    Poll::Ready(permit) => {
                        future.set(semaphore.clone().acquire_owned());
                        permit.ok()
                    }
                    Poll::Pending => return Poll::Pending,
                },
                None => None,
            };
            match accept.as_mut().poll(context) {
                Poll::Ready((item, this)) => {
                    accept.set(this.into_accept());
                    Poll::Ready(Some((item, permit)))
                }
                Poll::Pending => Poll::Pending,
            }
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    #[must_use]
    pub fn with_allowlist(mut self, allowlist: Option<Vec<IpNet>>) -> Self {
        self.origin_filter = allowlist;
        self
    }

    #[must_use]
    pub fn with_keepalive(mut self, keepalive: Option<TcpKeepaliveConfig>) -> Self {
        self.keepalive = keepalive;
        self
    }
}

impl From<TcpListener> for MaybeTlsListener {
    fn from(listener: TcpListener) -> Self {
        Self {
            listener,
            acceptor: None,
            origin_filter: None,
            keepalive: None,
        }
    }
}

pub struct MaybeTlsIncomingStream<S> {
    state: StreamState<S>,
    // BoxFuture doesn't allow access to the inner stream, but users
    // of MaybeTlsIncomingStream want access to the peer address while
    // still handshaking, so we have to cache it here.
    peer_addr: SocketAddr,
}

enum StreamState<S> {
    Accepted(MaybeTlsStream<S>),
    Accepting(BoxFuture<'static, Result<SslStream<S>, TlsError>>),
    AcceptError(String),
    Closed,
}

impl<S> MaybeTlsIncomingStream<S> {
    pub const fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// None if connection still hasn't been established.
    pub fn get_ref(&self) -> Option<&S> {
        use super::MaybeTls;

        match &self.state {
            StreamState::Accepted(stream) => Some(match stream {
                MaybeTls::Raw(s) => s,
                MaybeTls::Tls(s) => s.get_ref(),
            }),
            StreamState::Accepting(_) | StreamState::AcceptError(_) | StreamState::Closed => None,
        }
    }

    pub const fn ssl_stream(&self) -> Option<&SslStream<S>> {
        use super::MaybeTls;

        match &self.state {
            StreamState::Accepted(stream) => match stream {
                MaybeTls::Raw(_) => None,
                MaybeTls::Tls(s) => Some(s),
            },
            StreamState::Accepting(_) | StreamState::AcceptError(_) | StreamState::Closed => None,
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut S> {
        use super::MaybeTls;

        match &mut self.state {
            StreamState::Accepted(stream) => Some(match stream {
                MaybeTls::Raw(s) => s,
                MaybeTls::Tls(s) => s.get_mut(),
            }),
            StreamState::Accepting(_) | StreamState::AcceptError(_) | StreamState::Closed => None,
        }
    }
}

impl MaybeTlsIncomingStream<TcpStream> {
    pub(super) fn new(
        stream: TcpStream,
        peer_addr: SocketAddr,
        acceptor: Option<SslAcceptor>,
    ) -> Self {
        let state = match acceptor {
            Some(acceptor) => StreamState::Accepting(
                async move {
                    let ssl = Ssl::new(acceptor.context()).context(SslBuildSnafu)?;
                    let mut stream = SslStream::new(ssl, stream).context(SslBuildSnafu)?;
                    Pin::new(&mut stream)
                        .accept()
                        .await
                        .context(HandshakeSnafu)?;
                    Ok(stream)
                }
                .boxed(),
            ),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Self { state, peer_addr }
    }

    // Explicit handshake method
    pub async fn handshake(&mut self) -> crate::tls::Result<()> {
        if let StreamState::Accepting(fut) = &mut self.state {
            let stream = fut.await?;
            self.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
        }

        Ok(())
    }

    pub fn set_keepalive(&mut self, keepalive: TcpKeepaliveConfig) -> io::Result<()> {
        let stream = self.get_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "Can't set keepalive on connection that has not been accepted yet.",
            )
        })?;

        if let Some(time_secs) = keepalive.time_secs {
            let config =
                socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(time_secs));

            tcp::set_keepalive(stream, &config)?;
        }

        Ok(())
    }

    pub fn set_receive_buffer_bytes(&mut self, bytes: usize) -> std::io::Result<()> {
        let stream = self.get_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "Can't set receive buffer size on connection that has not been accepted yet.",
            )
        })?;

        tcp::set_receive_buffer_size(stream, bytes)
    }

    fn poll_io<T, F>(self: Pin<&mut Self>, cx: &mut Context, poll_fn: F) -> Poll<io::Result<T>>
    where
        F: FnOnce(Pin<&mut MaybeTlsStream<TcpStream>>, &mut Context) -> Poll<io::Result<T>>,
    {
        let this = self.get_mut();
        loop {
            return match &mut this.state {
                StreamState::Accepted(stream) => poll_fn(Pin::new(stream), cx),
                StreamState::Accepting(fut) => match std::task::ready!(fut.as_mut().poll(cx)) {
                    Ok(stream) => {
                        this.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
                        continue;
                    }
                    Err(error) => {
                        let error = io::Error::other(error);
                        this.state = StreamState::AcceptError(error.to_string());
                        Poll::Ready(Err(error))
                    }
                },
                StreamState::AcceptError(error) => {
                    Poll::Ready(Err(io::Error::other(error.clone())))
                }
                StreamState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            };
        }
    }
}

impl AsyncRead for MaybeTlsIncomingStream<TcpStream> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_io(cx, |s, cx| s.poll_read(cx, buf))
    }
}

impl AsyncWrite for MaybeTlsIncomingStream<TcpStream> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.poll_io(cx, |s, cx| s.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.poll_io(cx, AsyncWrite::poll_flush)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match &mut this.state {
            StreamState::Accepted(stream) => match Pin::new(stream).poll_shutdown(cx) {
                Poll::Ready(Ok(())) => {
                    this.state = StreamState::Closed;
                    Poll::Ready(Ok(()))
                }
                poll_result => poll_result,
            },
            StreamState::Accepting(fut) => match std::task::ready!(fut.as_mut().poll(cx)) {
                Ok(stream) => {
                    this.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
                    Poll::Pending
                }
                Err(error) => {
                    let error = io::Error::other(error);
                    this.state = StreamState::AcceptError(error.to_string());
                    Poll::Ready(Err(error))
                }
            },
            StreamState::AcceptError(error) => Poll::Ready(Err(io::Error::other(error.clone()))),
            StreamState::Closed => Poll::Ready(Ok(())),
        }
    }
}

#[derive(Debug)]
pub struct CertificateMetadata {
    pub country_name: Option<String>,
    pub state_or_province_name: Option<String>,
    pub locality_name: Option<String>,
    pub organization_name: Option<String>,
    pub organizational_unit_name: Option<String>,
    pub common_name: Option<String>,
}

impl CertificateMetadata {
    pub fn subject(&self) -> String {
        let mut components = Vec::<String>::with_capacity(6);
        if let Some(cn) = &self.common_name {
            components.push(format!("CN={cn}"));
        }
        if let Some(ou) = &self.organizational_unit_name {
            components.push(format!("OU={ou}"));
        }
        if let Some(o) = &self.organization_name {
            components.push(format!("O={o}"));
        }
        if let Some(l) = &self.locality_name {
            components.push(format!("L={l}"));
        }
        if let Some(st) = &self.state_or_province_name {
            components.push(format!("ST={st}"));
        }
        if let Some(c) = &self.country_name {
            components.push(format!("C={c}"));
        }
        components.join(",")
    }
}

impl From<X509> for CertificateMetadata {
    fn from(cert: X509) -> Self {
        let mut subject_metadata: HashMap<String, String> = HashMap::new();
        for entry in cert.subject_name().entries() {
            let data_string = entry.data().to_string().unwrap_or_default();
            subject_metadata.insert(entry.object().to_string(), data_string);
        }
        Self {
            country_name: subject_metadata.get("countryName").cloned(),
            state_or_province_name: subject_metadata.get("stateOrProvinceName").cloned(),
            locality_name: subject_metadata.get("localityName").cloned(),
            organization_name: subject_metadata.get("organizationName").cloned(),
            organizational_unit_name: subject_metadata.get("organizationalUnitName").cloned(),
            common_name: subject_metadata.get("commonName").cloned(),
        }
    }
}

#[derive(Clone)]
pub struct MaybeTlsConnectInfo {
    pub remote_addr: SocketAddr,
    pub peer_certs: Option<Vec<Certificate>>,
}

impl Connected for MaybeTlsIncomingStream<TcpStream> {
    type ConnectInfo = MaybeTlsConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        MaybeTlsConnectInfo {
            remote_addr: self.peer_addr(),
            peer_certs: self
                .ssl_stream()
                .and_then(|s| s.ssl().peer_cert_chain())
                .map(|s| {
                    s.into_iter()
                        .filter_map(|c| c.to_pem().ok())
                        .map(Certificate::from_pem)
                        .collect()
                }),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{TcpListener as StdTcpListener, TcpStream as StdTcpStream},
        sync::Mutex,
        thread,
    };

    use openssl::ssl::{SslConnector, SslFiletype, SslSession, SslSessionCacheMode};

    use super::*;
    use crate::tls::{
        TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH, TEST_PEM_CLIENT_KEY_PATH, TlsConfig,
    };

    /// What the server observed on one accepted connection.
    #[derive(Debug, PartialEq)]
    struct Accepted {
        session_reused: bool,
        has_peer_certificate: bool,
    }

    /// Serves `connections` TLS connections with an acceptor which verifies client certificates.
    fn serve_mtls(connections: usize) -> (u16, thread::JoinHandle<Vec<Result<Accepted, String>>>) {
        let config = TlsConfig {
            verify_certificate: Some(true),
            ..TlsConfig::test_config()
        };
        let acceptor = TlsSettings::from_options_base(Some(&config), true)
            .unwrap()
            .acceptor()
            .unwrap();
        let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            (0..connections)
                .map(|_| {
                    let (tcp, _) = listener.accept().unwrap();
                    let mut stream = acceptor.accept(tcp).map_err(|error| error.to_string())?;
                    let accepted = Accepted {
                        session_reused: stream.ssl().session_reused(),
                        has_peer_certificate: stream.ssl().peer_certificate().is_some(),
                    };
                    _ = stream.shutdown();
                    Ok(accepted)
                })
                .collect()
        });
        (port, server)
    }

    /// A client which presents a certificate and caches sessions like Vector's HTTP client.
    fn mtls_connector() -> (SslConnector, Arc<Mutex<Option<SslSession>>>) {
        let session = Arc::new(Mutex::new(None));
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_ca_file(TEST_PEM_CA_PATH).unwrap();
        builder
            .set_certificate_chain_file(TEST_PEM_CLIENT_CRT_PATH)
            .unwrap();
        builder
            .set_private_key_file(TEST_PEM_CLIENT_KEY_PATH, SslFiletype::PEM)
            .unwrap();
        builder.set_session_cache_mode(SslSessionCacheMode::CLIENT);
        builder.set_new_session_callback({
            let session = Arc::clone(&session);
            move |_, new_session| *session.lock().unwrap() = Some(new_session)
        });
        (builder.build(), session)
    }

    fn connect(
        connector: &SslConnector,
        port: u16,
        session: Option<&SslSession>,
    ) -> Result<bool, String> {
        let mut config = connector.configure().unwrap();
        if let Some(session) = session {
            // SAFETY: the session was created by a connection with the same `SslContext`.
            unsafe { config.set_session(session) }.unwrap();
        }
        let tcp = StdTcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut stream = config
            .connect("localhost", tcp)
            .map_err(|error| error.to_string())?;
        let session_reused = stream.ssl().session_reused();
        // Close cleanly: OpenSSL marks the session of an unclean close as not resumable.
        _ = stream.shutdown();
        Ok(session_reused)
    }

    #[test]
    fn mtls_session_resumes() {
        let (port, server) = serve_mtls(2);
        let (connector, cached_session) = mtls_connector();
        connect(&connector, port, None).unwrap();
        let session = cached_session
            .lock()
            .unwrap()
            .clone()
            .expect("server issued no session");

        let client_reused = connect(&connector, port, Some(&session));

        let accepted = server.join().unwrap();
        assert_eq!(client_reused, Ok(true));
        assert_eq!(
            accepted[1],
            Ok(Accepted {
                session_reused: true,
                has_peer_certificate: true,
            })
        );
    }

    #[test]
    fn certificate_metadata_full() {
        let example_meta = CertificateMetadata {
            common_name: Some("common".to_owned()),
            country_name: Some("country".to_owned()),
            locality_name: Some("locality".to_owned()),
            organization_name: Some("organization".to_owned()),
            organizational_unit_name: Some("org_unit".to_owned()),
            state_or_province_name: Some("state".to_owned()),
        };

        let expected = format!(
            "CN={},OU={},O={},L={},ST={},C={}",
            example_meta.common_name.as_ref().unwrap(),
            example_meta.organizational_unit_name.as_ref().unwrap(),
            example_meta.organization_name.as_ref().unwrap(),
            example_meta.locality_name.as_ref().unwrap(),
            example_meta.state_or_province_name.as_ref().unwrap(),
            example_meta.country_name.as_ref().unwrap()
        );
        assert_eq!(expected, example_meta.subject());
    }

    #[test]
    fn certificate_metadata_partial() {
        let example_meta = CertificateMetadata {
            common_name: Some("common".to_owned()),
            country_name: Some("country".to_owned()),
            locality_name: None,
            organization_name: Some("organization".to_owned()),
            organizational_unit_name: Some("org_unit".to_owned()),
            state_or_province_name: None,
        };

        let expected = format!(
            "CN={},OU={},O={},C={}",
            example_meta.common_name.as_ref().unwrap(),
            example_meta.organizational_unit_name.as_ref().unwrap(),
            example_meta.organization_name.as_ref().unwrap(),
            example_meta.country_name.as_ref().unwrap()
        );
        assert_eq!(expected, example_meta.subject());
    }
}
