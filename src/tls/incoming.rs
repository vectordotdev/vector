use super::*;
use futures01::Poll;
use futures01::{try_ready, Async, Future, Stream};
use openssl::ssl::SslAcceptor;
use snafu::ResultExt;
use std::fmt::{self, Debug, Formatter};
use std::net::SocketAddr;
use tokio::net::{tcp::Incoming, TcpListener, TcpStream};
use tokio_openssl::{AcceptAsync, SslAcceptorExt, SslStream};

pub(crate) type MaybeTlsStream<S> = MaybeTls<S, SslStream<S>>;

pub(crate) struct MaybeTlsIncoming<I: Stream> {
    incoming: I,
    acceptor: Option<SslAcceptor>,
    state: MaybeTlsIncomingState<I::Item>,
}

enum MaybeTlsIncomingState<S> {
    Inner,
    Accepting(AcceptAsync<S>),
}

impl<I: Stream> MaybeTlsIncoming<I> {
    pub(crate) fn new(incoming: I, acceptor: Option<SslAcceptor>) -> crate::Result<Self> {
        Ok(Self {
            incoming,
            acceptor,
            state: MaybeTlsIncomingState::Inner,
        })
    }
}

impl Stream for MaybeTlsIncoming<Incoming> {
    type Item = MaybeTlsStream<TcpStream>;
    type Error = TlsError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match &mut self.state {
                MaybeTlsIncomingState::Inner => {
                    let stream = if let Some(stream) = try_ready!(self
                        .incoming
                        .poll()
                        .map_err(Into::into)
                        .context(IncomingListener))
                    {
                        stream
                    } else {
                        return Ok(Async::Ready(None));
                    };

                    if let Some(acceptor) = &mut self.acceptor {
                        let fut = acceptor.accept_async(stream);

                        self.state = MaybeTlsIncomingState::Accepting(fut);
                        continue;
                    } else {
                        return Ok(Async::Ready(Some(MaybeTlsStream::Raw(stream))));
                    }
                }

                MaybeTlsIncomingState::Accepting(fut) => {
                    let stream = try_ready!(fut.poll().context(Handshake));
                    self.state = MaybeTlsIncomingState::Inner;

                    return Ok(Async::Ready(Some(MaybeTlsStream::Tls(stream))));
                }
            }
        }
    }
}

impl<I: Stream + Debug> Debug for MaybeTlsIncoming<I> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("MaybeTlsIncoming")
            .field("incoming", &self.incoming)
            .finish()
    }
}

impl TlsSettings {
    pub(crate) fn acceptor(&self) -> crate::Result<SslAcceptor> {
        match self.identity {
            None => Err(TlsError::MissingRequiredIdentity.into()),
            Some(_) => {
                let mut acceptor =
                    SslAcceptor::mozilla_intermediate(SslMethod::tls()).context(CreateAcceptor)?;
                self.apply_context(&mut acceptor)?;
                Ok(acceptor.build())
            }
        }
    }
}

impl MaybeTlsSettings {
    pub(crate) fn bind(&self, addr: &SocketAddr) -> crate::Result<MaybeTlsIncoming<Incoming>> {
        let listener = TcpListener::bind(addr)?;
        let incoming = listener.incoming();

        let acceptor = match self {
            Self::Tls(tls) => Some(tls.acceptor()?.into()),
            Self::Raw(()) => None,
        };

        MaybeTlsIncoming::new(incoming, acceptor)
    }
}
