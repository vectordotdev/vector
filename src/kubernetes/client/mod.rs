//! A Kubernetes API client built using Vector interfaces to the system
//! resources as building blocks.
//!
//! Here are a few pointers to the resources that were used as an inspiration
//! for this mod:
//!
//! - https://github.com/kubernetes/client-go/blob/master/tools/clientcmd/api/types.go
//!
//!   A part of the official Kubernetes client library (in Go) that contains
//!   the structure for KUBECONFIG files. Used for reference on naming things.
//!
//! - https://github.com/kubernetes/apimachinery/blob/master/pkg/watch/watch.go
//!
//!   The reference design of the watchers composition and interfaces that's
//!   known to work.
//!
//! - https://github.com/kubernetes/client-go/blob/master/rest/config.go
//!
//!   The reference implementation on preparing the in-cluster config.
//!

use crate::{dns::Resolver, sinks::util::http::HttpClient, tls::TlsSettings};
use async_trait::async_trait;
use http::{
    header::{self, HeaderValue},
    uri, Request, Response, Uri,
};
use hyper::body::Body;
use k8s_runtime::Client as RuntimeClient;

pub mod config;

use config::Config;

/// A client to the k8s API.
///
/// Wraps our in-house [`HttpClient`].
#[derive(Debug, Clone)]
pub struct Client {
    inner: HttpClient,
    uri_scheme: uri::Scheme,
    uri_authority: uri::Authority,
    auth_header: HeaderValue,
}

impl Client {
    /// Create a new [`Client`].
    ///
    /// Takes the common kubernetes API cluster configuration [`Config`] and
    /// a [`Resolver`] that is generally not the part of the config, but is
    /// specific to our [`HttpClient`] implementation.
    ///
    /// Consumes the configuration to populate the internal state.
    /// Retunrs an error if the configuratiton is not valid.
    // TODO: add a proper error type.
    pub fn new(config: Config, resolver: Resolver) -> crate::Result<Self> {
        let Config {
            base,
            tls_options,
            token,
        } = config;

        let tls_settings = TlsSettings::from_options(&Some(tls_options))?;
        let inner = HttpClient::new(resolver, tls_settings)?;

        let uri::Parts {
            scheme, authority, ..
        } = base.into_parts();

        let uri_scheme = scheme.ok_or_else(|| "no scheme")?;
        let uri_authority = authority.ok_or_else(|| "no authority")?;

        let auth_header = format!("Bearer {}", token);
        let auth_header = HeaderValue::from_str(auth_header.as_str())?;

        Ok(Self {
            inner,
            uri_scheme,
            uri_authority,
            auth_header,
        })
    }

    fn prepare_request<B: Into<Body>>(&self, req: Request<B>) -> Request<Body> {
        let (mut parts, body) = req.into_parts();
        let body = body.into();

        parts.uri = self.adjust_uri(parts.uri);
        parts
            .headers
            .insert(header::AUTHORIZATION, self.auth_header.clone());

        Request::from_parts(parts, body)
    }

    fn adjust_uri(&self, uri: Uri) -> Uri {
        let mut parts = uri.into_parts();
        parts.scheme = Some(self.uri_scheme.clone());
        parts.authority = Some(self.uri_authority.clone());
        Uri::from_parts(parts).unwrap()
    }
}

#[async_trait]
impl RuntimeClient for Client {
    type Body = Body;
    type Error = Error;

    /// Alters a request according to the client configuraion and sends it.
    async fn send<B>(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Self::Error>
    where
        B: Into<Self::Body> + Send,
    {
        let req = self.prepare_request(req);
        Ok(self.inner.send(req).await?)
    }
}

/// `Box<dyn Error>` doesn't implement `Error`, so we need this simple wrapper.
#[derive(Debug)]
#[repr(transparent)]
pub struct Error(pub crate::Error);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0.as_ref(), f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<crate::Error> for Error {
    fn from(val: crate::Error) -> Self {
        Self(val)
    }
}
