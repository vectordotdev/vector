use crate::{
    dns::Resolver,
    event::{Event, LogEvent, Value},
    runtime::TaskExecutor,
    sinks::util::{
        http::https_client,
        tls::{TlsOptions, TlsSettings},
    },
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use bytes::Bytes;
use evmap;
use futures::stream::Stream;
use futures03::compat::Future01CompatExt;
use http::{
    header,
    uri::{self, Scheme},
    Request, Uri,
};
use hyper::client::HttpConnector;
use hyper::Body;
use hyper_tls::HttpsConnector;
use k8s_openapi::{
    self as k8s,
    api::core::v1 as api,
    api::core::v1::{Pod, WatchPodForAllNamespacesResponse},
    apimachinery::pkg::apis::meta::v1::WatchEvent,
    Response, ResponseError,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::fs;
use string_cache::DefaultAtom as Atom;

/// Kubernetes client which watches for changes of T on one Kubernetes API endpoint.
pub struct WatchClient<T: Response> {
    /// Must add:
    ///  - uri
    ///  - resource_version
    ///  - watch field
    /// This can be achieved with for example `Pod::watch_pod_for_all_namespaces`.
    request_builder: Box<
        dyn Fn(Option<Version>) -> Result<Request<Vec<u8>>, BuildError> + Send + Sync + 'static,
    >,
    updater: Box<dyn FnMut(T) -> WatchResult + Send + Sync + 'static>,
    token: Option<String>,
    host: String,
    port: String,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
}

impl<T: Response> WatchClient<T> {
    pub fn new(
        resolver: Resolver,
        token: Option<String>,
        host: String,
        port: String,
        tls_settings: TlsSettings,
        request_builder: impl Fn(Option<Version>) -> Result<Request<Vec<u8>>, BuildError>
            + Send
            + Sync
            + 'static,
        updater: impl FnMut(T) -> WatchResult + Send + Sync + 'static,
    ) -> Result<Self, BuildError> {
        let this = Self {
            request_builder: Box::new(request_builder) as Box<_>,
            updater: Box::new(updater) as Box<_>,
            token,
            host,
            port,
            client: https_client(resolver, tls_settings).context(HttpError)?,
        };

        // Test now if the only other source of errors passes.
        this.watch_pods_request(None)?;

        Ok(this)
    }

    /// Watches for metadata changes and propagates them to updater.
    /// Never returns
    pub async fn run(&mut self) {
        // If watch is initiated with None resource_version, we will receive initial
        // list of pods as synthetic "Added" events.
        // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
        let mut version = None;

        // Restarts watch request
        loop {
            // We could clear Metadata map at this point, as Kubernets documentation suggests,
            // but then we would have a time gap during which events wouldn't be enriched
            // with metadata.
            let request = self
                .watch_pods_request(version.clone())
                .expect("Request succesfully builded before");
            version = self.watch(version, request).await;
        }
    }

    /// Watches for pods metadata with given watch request.
    async fn watch(
        &mut self,
        mut version: Option<Version>,
        request: Request<Body>,
    ) -> Option<Version> {
        // Start watching
        let response = self.client.request(request).compat().await;
        match response {
            Ok(response) => {
                info!(message = "Watching Pod list for changes.");
                let status = response.status();
                let mut unused = Vec::new();
                let mut body = response.into_body();
                'watch: loop {
                    // We need to process Chunks as they come because watch behaves like
                    // a never ending stream of Chunks.
                    match body.into_future().compat().await {
                        Ok((chunk, tmp_body)) => {
                            body = tmp_body;

                            if let Some(chunk) = chunk {
                                unused.extend_from_slice(chunk.as_ref());

                                // Parse then process, recieved unused data
                                'process: loop {
                                    match T::try_from_parts(
                                        status, &unused,
                                    ) {
                                        Ok((data, used_bytes)) => {
                                            // Process watch event
                                            match (self.updater)(data) {
                                                WatchResult::New(new_version) => {
                                                    // Store last resourceVersion
                                                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes
                                                    version = new_version.or(version);

                                                    assert!(
                                                        used_bytes > 0,
                                                        "Parser must consume some data"
                                                    );

                                                    let _ = unused.drain(..used_bytes);
                                                    continue 'process;
                                                }
                                                WatchResult::Reload => (),
                                                WatchResult::Restart => return None,
                                            }
                                        }
                                        Err(ResponseError::NeedMoreData) => continue 'watch,
                                        Err(error) => debug!(
                                            "Unable to parse WatchPodForAllNamespacesResponse from response. Error: {:?}",
                                            error
                                        ),
                                    }
                                    break 'watch;
                                }
                            }
                        }
                        Err(error) => debug!(message = "Watch request failed.", ?error),
                    }
                    break 'watch;
                }
            }
            Err(error) => debug!(message = "Failed resolving request.", ?error),
        }

        version
    }

    // Builds request to watch pods.
    fn watch_pods_request(
        &self,
        resource_version: Option<Version>,
    ) -> Result<Request<Body>, BuildError> {
        // Prepare request
        let mut request = (self.request_builder)(resource_version)?;

        self.authorize(&mut request)?;
        self.fill_uri(&mut request)?;

        let (parts, body) = request.into_parts();
        Ok(Request::from_parts(parts, body.into()))
    }

    fn authorize(&self, request: &mut Request<Vec<u8>>) -> Result<(), BuildError> {
        if let Some(token) = self.token.as_ref() {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(format!("Bearer {}", token).as_str())
                    .context(InvalidToken)?,
            );
        }

        Ok(())
    }

    fn fill_uri(&self, request: &mut Request<Vec<u8>>) -> Result<(), BuildError> {
        let mut uri = request.uri().clone().into_parts();
        uri.scheme = Some(Scheme::HTTPS);
        uri.authority = Some(
            format!("{}:{}", self.host, self.port)
                .parse()
                .context(InvalidUri)?,
        );
        *request.uri_mut() = Uri::from_parts(uri).context(InvalidUriParts)?;

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Http client construction errored {}.", source))]
    HttpError { source: crate::Error },
    #[snafu(display("Uri is invalid: {}.", source))]
    InvalidUri { source: uri::InvalidUri },
    #[snafu(display("Uri is invalid: {}.", source))]
    InvalidUriParts { source: uri::InvalidUriParts },
    #[snafu(display("Authorization token is invalid: {}.", source))]
    InvalidToken { source: header::InvalidHeaderValue },
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
pub struct Version(String);

#[derive(Clone, Debug)]
pub enum WatchResult {
    /// Potentialy newer version
    New(Option<Version>),
    /// Start new request with current version.
    Reload,
    /// Start new request with None version.
    Restart,
}
