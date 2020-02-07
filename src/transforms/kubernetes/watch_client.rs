use crate::{
    dns::Resolver,
    sinks::util::{
        http::https_client,
        tls::{TlsOptions, TlsSettings},
    },
};
use futures::stream::Stream;
use futures03::compat::Future01CompatExt;
use http::{header, status::StatusCode, uri, Request, Uri};
use hyper::client::HttpConnector;
use hyper::Body;
use hyper_tls::HttpsConnector;
use k8s_openapi::{
    api::core::v1::{Pod, WatchPodForAllNamespacesResponse},
    apimachinery::pkg::apis::meta::v1::WatchEvent,
    RequestError, Response, ResponseError, WatchOptional,
};
use snafu::{ResultExt, Snafu};

/// Config which could be loaded from kubeconfig or local kubernetes cluster.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    resolver: Resolver,
    token: Option<String>,
    server: Uri,
    tls_settings: TlsSettings,
}

impl ClientConfig {
    // NOTE: Currently used only for tests, but can be later used in
    //       other places, but then the unsupported feature should be
    //       implemented.
    /// Loads configuration from local kubeconfig file, the same
    /// one that kubectl uses.
    #[cfg(test)]
    pub fn load_kube_config(resolver: Resolver) -> Option<Self> {
        let config = super::kube_config::load_kube_config()?;

        // Get current context
        let context = &config
            .contexts
            .iter()
            .find(|context| context.name == config.current_context)?
            .context;

        // Get current user
        let user = &config
            .users
            .iter()
            .find(|user| user.name == context.user)?
            .user;

        // Get current cluster
        let cluster = &config
            .clusters
            .iter()
            .find(|cluster| cluster.name == context.cluster)?
            .cluster;

        // The not yet supported features
        assert!(user.username.is_none(), "Not yet supported");
        assert!(user.password.is_none(), "Not yet supported");
        assert!(user.token_file.is_none(), "Not yet supported");
        assert!(
            cluster.certificate_authority_data.is_none(),
            "Not yet supported"
        );
        assert!(user.client_certificate.is_none(), "Not yet supported");
        assert!(user.client_certificate_data.is_none(), "Not yet supported");
        assert!(user.client_key.is_none(), "Not yet supported");
        assert!(user.client_key_data.is_none(), "Not yet supported");

        // Construction
        Some(ClientConfig {
            resolver,
            token: user.token.clone(),
            server: Uri::from_str(&cluster.server).unwrap(),
            tls_settings: TlsSettings::from_options(Some(TlsOptions {
                verify_certificate: cluster.insecure_skip_tls_verify,
                ca_path: cluster.certificate_authority.clone().map(PathBuf::from)?,
                ..TlsOptions::default()
            })),
        })
    }

    /// Creates new watcher who will call updater function with freshest Pod data.
    /// Request to API server is made with given WatchOptional.
    pub fn build_pod_watch(
        self,
        request_optional: WatchOptional<'static>,
        mut updater: impl FnMut(&Pod) + Send + Sync + 'static,
    ) -> Result<WatchClient<WatchPodForAllNamespacesResponse>, BuildError> {
        let request_builder = move |version: Option<&Version>| {
            Pod::watch_pod_for_all_namespaces(WatchOptional {
                resource_version: version.map(|v| v.0.as_str()),
                ..request_optional.clone()
            })
            .map(|(req, _)| req)
            .context(K8SOpenapiError)
        };

        let updater = move |response| {
            WatchResult::New(Some(response))
                .then_response_to_event()
                .then_event_to_data()
                .peek(|pod| updater(pod))
                .map(|pod| {
                    pod.metadata
                        .as_ref()
                        .and_then(|metadata| metadata.resource_version.clone().map(Version))
                })
        };

        self.build(request_builder, updater)
    }

    /// Should be used by other build_* functions which hide request_builder, and
    /// simplify updater function.
    fn build<T: Response>(
        self,
        request_builder: impl Fn(Option<&Version>) -> Result<Request<Vec<u8>>, BuildError>
            + Send
            + Sync
            + 'static,
        updater: impl FnMut(T) -> WatchResult<Version> + Send + Sync + 'static,
    ) -> Result<WatchClient<T>, BuildError> {
        let client =
            https_client(self.resolver.clone(), self.tls_settings.clone()).context(HttpError)?;

        let client = WatchClient::<T> {
            request_builder: Box::new(request_builder) as Box<_>,
            updater: Box::new(updater) as Box<_>,
            client,
            config: self,
        };

        // Test now if the only other source of errors passes.
        client.build_request(None)?;

        Ok(client)
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
        let mut uri = self.server.clone().into_parts();
        uri.path_and_query = request.uri().clone().into_parts().path_and_query;
        *request.uri_mut() = Uri::from_parts(uri).context(InvalidUriParts)?;
        Ok(())
    }
}

/// Kubernetes client which watches for changes of T on one Kubernetes API endpoint.
pub struct WatchClient<T: Response> {
    /// Must add:
    ///  - uri
    ///  - resource_version
    ///  - watch field
    /// This can be achieved with for example `Pod::watch_pod_for_all_namespaces`.
    request_builder: Box<
        dyn Fn(Option<&Version>) -> Result<Request<Vec<u8>>, BuildError> + Send + Sync + 'static,
    >,
    updater: Box<dyn FnMut(T) -> WatchResult<Version> + Send + Sync + 'static>,
    config: ClientConfig,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
}

impl<T: Response> WatchClient<T> {
    /// Watches for data changes and propagates them to updater.
    /// Never returns
    pub async fn run(&mut self) {
        // If watch is initiated with None resource_version, we will receive initial
        // list of data as synthetic "Added" events.
        // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
        let mut version = None;

        loop {
            let request = self
                .build_request(version.clone())
                .expect("Request succesfully builded before");

            // Restarts watch with new request.
            version = self.watch(version, request).await;
        }
    }

    /// Watches for data with given watch request.
    /// Returns resource version from which watching can start.
    /// Accepts resource version from which request is starting to watch.
    async fn watch(
        &mut self,
        mut version: Option<Version>,
        request: Request<Body>,
    ) -> Option<Version> {
        // Start watching
        let response = self.client.request(request).compat().await;
        match response {
            Ok(response) => {
                info!(message = "Watching list for changes.");
                let status = response.status();
                if status == StatusCode::OK {
                    // Connected

                    let mut unused = Vec::new();
                    let mut body = response.into_body();
                    loop {
                        // Wait for responses from the API server.
                        match body.into_future().compat().await {
                            Ok((Some(chunk), tmp_body)) => {
                                // Append new data to unused.
                                unused.extend_from_slice(chunk.as_ref());
                                body = tmp_body;

                                // We need to process unused data as soon as we get
                                // new data, because a watch on Kubernetes object behaves
                                // like a never ending stream of bytes.
                                match self.process_buffer(version, &mut unused) {
                                    WatchResult::New(new_version) => {
                                        version = new_version;

                                        //Continue watching.
                                        continue;
                                    }
                                    WatchResult::Reload(new_version) => return new_version,
                                    WatchResult::Restart => return None,
                                }
                            }
                            Ok((None, _)) => debug!("Watch connection unexpectedly ended."),
                            Err(error) => debug!(message = "Watch request failed.", ?error),
                        }
                        break;
                    }
                } else {
                    debug!(message="Status of response is not 200 OK.",%status);
                }
            }
            Err(error) => debug!(message = "Failed resolving request.", ?error),
        }

        version
    }

    /// Buffer contains unused data.
    /// Removes from buffer used data.
    /// StatusCode should be 200 OK.
    fn process_buffer(
        &mut self,
        mut version: Option<Version>,
        unused: &mut Vec<u8>,
    ) -> WatchResult<Version> {
        // Parse then process recieved unused data.
        loop {
            return match T::try_from_parts(StatusCode::OK, &unused) {
                Ok((data, used_bytes)) => {
                    assert!(used_bytes > 0, "Parser must consume some data");
                    // Remove used data.
                    let _ = unused.drain(..used_bytes);

                    // Process watch event
                    // Store last resourceVersion
                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes
                    match (self.updater)(data) {
                        WatchResult::New(new_version) => {
                            version = new_version.or(version);
                            // Continue parsing out data.
                            continue;
                        }
                        WatchResult::Reload(new_version) => {
                            WatchResult::Reload(new_version.or(version))
                        }
                        WatchResult::Restart => WatchResult::Restart,
                    }
                }
                Err(ResponseError::NeedMoreData) => WatchResult::New(version),
                Err(error) => {
                    debug!(
                        "Unable to parse {} from response. Error: {:?}",
                        std::any::type_name::<T>(),
                        error
                    );
                    WatchResult::Reload(version)
                }
            };
        }
    }

    // Builds request to watch data.
    fn build_request(&self, version: Option<Version>) -> Result<Request<Body>, BuildError> {
        // Prepare request
        let mut request = (self.request_builder)(version.as_ref())?;

        self.config.authorize(&mut request)?;
        self.config.fill_uri(&mut request)?;

        let (parts, body) = request.into_parts();
        Ok(Request::from_parts(parts, body.into()))
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Http client construction errored {}.", source))]
    HttpError { source: crate::Error },
    #[snafu(display("Failed constructing request: {}.", source))]
    K8SOpenapiError { source: RequestError },
    #[snafu(display("Uri is invalid: {}.", source))]
    InvalidUri { source: uri::InvalidUri },
    #[snafu(display("Uri is invalid: {}.", source))]
    InvalidUriParts { source: uri::InvalidUriParts },
    #[snafu(display("Authorization token is invalid: {}.", source))]
    InvalidToken { source: header::InvalidHeaderValue },
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
struct Version(String);

/// Data over which various transformations are applied in sequence.
/// Transformations are short circuted on all cases except on New(Some(_)).
#[derive(Clone, Debug)]
enum WatchResult<T> {
    /// Everything is Ok path.
    /// Potentialy some data.
    New(Option<T>),
    /// Start new request with current version.
    Reload(Option<Version>),
    /// Start new request with None version.
    Restart,
}

impl<T> WatchResult<T> {
    /// Applies function if data exists.
    fn and_then<R>(self, map: impl FnOnce(T) -> WatchResult<R>) -> WatchResult<R> {
        match self {
            WatchResult::New(Some(data)) => map(data),
            WatchResult::New(None) => WatchResult::New(None),
            WatchResult::Reload(version) => WatchResult::Reload(version),
            WatchResult::Restart => WatchResult::Restart,
        }
    }

    /// Maps data if data exists.
    fn map<R>(self, map: impl FnOnce(T) -> Option<R>) -> WatchResult<R> {
        self.and_then(move |data| WatchResult::New(map(data)))
    }

    /// Peeks at existing data.
    fn peek(self, fun: impl FnOnce(&T)) -> Self {
        if let WatchResult::New(Some(ref data)) = &self {
            fun(data);
        }
        self
    }
}

impl WatchResult<WatchPodForAllNamespacesResponse> {
    /// Processes WatchPodForAllNamespacesResponse into WatchEvent<Pod>.
    fn then_response_to_event(self) -> WatchResult<WatchEvent<Pod>> {
        self.and_then(|response| match response {
            WatchPodForAllNamespacesResponse::Ok(event) => WatchResult::New(Some(event)),
            WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                debug!(message = "Received wrong object from Kubernetes API.");
                WatchResult::New(None)
            }
            WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                debug!(message = "Failed parsing watch event for Pods.", ?error);
                WatchResult::Reload(None)
            }
        })
    }
}

impl<T> WatchResult<WatchEvent<T>> {
    /// Processes WatchEvent<T> into T.
    fn then_event_to_data(self) -> WatchResult<T> {
        self.and_then(|event| match event {
            WatchEvent::Added(data)
            | WatchEvent::Modified(data)
            | WatchEvent::Bookmark(data)
            | WatchEvent::Deleted(data) => WatchResult::New(Some(data)),
            WatchEvent::ErrorStatus(status) => {
                if status.code == Some(410) {
                    // 410 Gone, restart with new list.
                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#410-gone-responses
                    warn!(message = "Watch list desynced. Restarting watch.", cause = ?status);
                    WatchResult::Restart
                } else {
                    debug!("Watch event with error status: {:?}.", status);
                    WatchResult::New(None)
                }
            }
            WatchEvent::ErrorOther(value) => {
                debug!(message="Encountered unknown error while watching.",error = ?value);
                WatchResult::New(None)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Resolver, TlsSettings, WatchClient, WatchOptional};

    #[test]
    fn buildable() {
        let rt = crate::runtime::Runtime::new().unwrap();
        ClientConfig {
            resolver: Resolver::new(Vec::new(), rt.executor()).unwrap(),
            token: None,
            host: "localhost".to_owned(),
            port: "8001".to_owned(),
            tls_settings: TlsSettings::from_options(&None).unwrap(),
        }
        .build_pod_watch(WatchOptional::default(), |_| ())
        .unwrap();
    }
}
