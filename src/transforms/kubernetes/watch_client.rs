use crate::{
    dns::Resolver,
    sinks::util::{http::https_client, tls::TlsSettings},
};
use futures::stream::Stream;
use futures03::compat::Future01CompatExt;
use http::{header, status::StatusCode, uri, Request, Uri};
use hyper::{client::HttpConnector, Body};
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
    /// Creates new watcher who will call updater function with freshest Pod data.
    /// Request to API server is made with given WatchOptional.
    pub fn build_pod_watch(
        self,
        request_optional: WatchOptional<'static>,
        mut updater: impl FnMut(&Pod) + Send + 'static,
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
            let pod = Self::event_to_data(Self::response_to_event(response)?)?;
            updater(&pod);
            Ok(pod
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.resource_version.clone().map(Version)))
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
        updater: impl FnMut(T) -> Result<Option<Version>, RuntimeError> + Send + 'static,
    ) -> Result<WatchClient<T>, BuildError> {
        let client =
            https_client(self.resolver.clone(), self.tls_settings.clone()).context(HttpError)?;

        let client = WatchClient::<T> {
            request_builder: Box::new(request_builder) as Box<_>,
            updater: Box::new(updater) as Box<_>,
            client,
            config: self,
            // If watch is initiated with None resource_version, we will receive initial
            // list of data as synthetic "Added" events.
            // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
            version: None,
        };

        // Test now if the only other source of errors passes.
        client.build_request()?;

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

    /// Processes WatchPodForAllNamespacesResponse into WatchEvent<Pod>.
    fn response_to_event(
        response: WatchPodForAllNamespacesResponse,
    ) -> Result<WatchEvent<Pod>, RuntimeError> {
        match response {
            WatchPodForAllNamespacesResponse::Ok(event) => Ok(event),
            WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                Err(RuntimeError::WrongObjectInResponse)
            }
            WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                Err(error).context(ResponseParseError)
            }
        }
    }

    /// Processes WatchEvent<T> into T.
    fn event_to_data<T>(event: WatchEvent<T>) -> Result<T, RuntimeError> {
        match event {
            WatchEvent::Added(data)
            | WatchEvent::Modified(data)
            | WatchEvent::Bookmark(data)
            | WatchEvent::Deleted(data) => Ok(data),
            WatchEvent::ErrorStatus(status) => Err(RuntimeError::WatchEventError { status }),
            WatchEvent::ErrorOther(other) => Err(RuntimeError::UnknownWatchEventError { other }),
        }
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
    updater: Box<dyn FnMut(T) -> Result<Option<Version>, RuntimeError> + Send + 'static>,
    config: ClientConfig,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
    /// Most recent watched resource version.
    version: Option<Version>,
}

impl<T: Response> WatchClient<T> {
    /// Watches for data changes and propagates them to updater.
    /// Never returns
    pub async fn run(&mut self) {
        loop {
            let request = self
                .build_request()
                .expect("Request succesfully builded before");

            // Restarts watch with new request.
            match self.watch(request).await {
                Ok(()) => (),
                Err(RuntimeError::WatchEventError { status }) if status.code == Some(410) => {
                    // 410 Gone, restart with new list.
                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#410-gone-responses
                    info!(
                        message = "Watch list desynced for Kubernetes Object. Restarting watch.",
                        object = std::any::type_name::<T>()
                    );
                    self.version = None;
                }
                Err(error) => debug!(%error),
            }
        }
    }

    /// Watches for data with given watch request.
    /// Returns resource version from which watching can start.
    /// Accepts resource version from which request is starting to watch.
    async fn watch(&mut self, request: Request<Body>) -> Result<(), RuntimeError> {
        // Start watching
        let response = self
            .client
            .request(request)
            .compat()
            .await
            .context(FailedConnecting)?;

        let status = response.status();
        if status == StatusCode::OK {
            // Connected succesfully
            info!(message = "Watching for changes.");

            let mut unused = Vec::new();
            let mut body = response.into_body();
            loop {
                // Wait for responses from the API server.
                let (chunk, tmp_body) = body
                    .into_future()
                    .compat()
                    .await
                    .map_err(|(error, _)| error)
                    .context(WatchConnectionErrored)?;

                body = tmp_body;
                let chunk = chunk.ok_or(RuntimeError::WatchUnexpectedlyEnded)?;

                // Append new data to unused.
                unused.extend_from_slice(chunk.as_ref());

                // We need to process unused data as soon as we get
                // new them. Because a watch on Kubernetes object behaves
                // like a never ending stream of bytes.
                self.process_unused(&mut unused)?;

                //Continue watching.
            }
        } else {
            Err(RuntimeError::ConnectionStatusNotOK { status })
        }
    }

    /// Decodes T from unused data and processes it further.
    /// Repeats decoding so long as there is sufficient data.
    /// Removes used data.
    /// StatusCode should be 200 OK.
    fn process_unused(&mut self, unused: &mut Vec<u8>) -> Result<(), RuntimeError> {
        // Parse then process recieved data.
        loop {
            match T::try_from_parts(StatusCode::OK, &unused) {
                Ok((data, used_bytes)) => {
                    assert!(used_bytes > 0, "Parser must consume some data");
                    // Remove used data.
                    let _ = unused.drain(..used_bytes);

                    // Process watch event
                    let new_version = (self.updater)(data)?;
                    // Store last resourceVersion
                    // https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes
                    self.version = new_version.or(self.version.take());

                    // Continue parsing out data.
                }
                Err(ResponseError::NeedMoreData) => return Ok(()),
                Err(error) => {
                    return Err(RuntimeError::ParseResponseError {
                        name: std::any::type_name::<T>().to_owned(),
                        error,
                    })
                }
            };
        }
    }

    // Builds request to watch data.
    fn build_request(&self) -> Result<Request<Body>, BuildError> {
        // Prepare request
        let mut request = (self.request_builder)(self.version.as_ref())?;

        self.config.authorize(&mut request)?;
        self.config.fill_uri(&mut request)?;

        let (parts, body) = request.into_parts();
        Ok(Request::from_parts(parts, body.into()))
    }
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
struct Version(String);

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

#[derive(Debug, Snafu)]
enum RuntimeError {
    #[snafu(display("Received wrong object from Kubernetes API."))]
    WrongObjectInResponse,
    #[snafu(display("Failed parsing response: {}.", source))]
    ResponseParseError { source: serde_json::error::Error },
    #[snafu(display("Watch event error with status: {:?}.", status))]
    WatchEventError {
        status: k8s_openapi::apimachinery::pkg::apis::meta::v1::Status,
    },
    #[snafu(display("Encountered unknown error while watching: {:?}.", other))]
    UnknownWatchEventError {
        other: k8s_openapi::apimachinery::pkg::runtime::RawExtension,
    },
    #[snafu(display("Unable to parse {} from response. Error: {}", name, error))]
    ParseResponseError {
        name: String,
        error: k8s_openapi::ResponseError,
    },
    #[snafu(display("Failed connecting to server: {}.", source))]
    FailedConnecting { source: hyper::Error },
    #[snafu(display("Status of response is not 200 OK, but: {}.", status))]
    ConnectionStatusNotOK { status: StatusCode },
    #[snafu(display("Watch connection unexpectedly ended."))]
    WatchUnexpectedlyEnded,
    #[snafu(display("Watch connection errored: {}", source))]
    WatchConnectionErrored { source: hyper::Error },
}

#[cfg(test)]
mod tests {
    use super::ClientConfig;
    use crate::{dns::Resolver, sinks::util::tls::TlsSettings};
    use http::Uri;
    use k8s_openapi::WatchOptional;

    #[test]
    fn buildable() {
        let rt = crate::runtime::Runtime::new().unwrap();
        ClientConfig {
            resolver: Resolver::new(Vec::new(), rt.executor()).unwrap(),
            token: None,
            server: Uri::from_static("https://localhost:8001"),
            tls_settings: TlsSettings::from_options(&None).unwrap(),
        }
        .build_pod_watch(WatchOptional::default(), |_| ())
        .unwrap();
    }
}

#[cfg(test)]
mod kube_tests {
    #![cfg(feature = "kubernetes-integration-tests")]

    use super::ClientConfig;
    use crate::{
        dns::Resolver,
        sinks::util::tls::TlsOptions,
        sinks::util::tls::TlsSettings,
        sources::kubernetes::test::{echo, Kube},
        test_util::{runtime, temp_file},
        transforms::kubernetes::kube_config,
    };
    use http::Uri;
    use k8s_openapi::WatchOptional;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::{path::PathBuf, str::FromStr, sync::mpsc::channel, time::Duration};
    use uuid::Uuid;

    fn store_to_file(data: &[u8]) -> Result<PathBuf, std::io::Error> {
        let path = temp_file();

        let mut file = OpenOptions::new().write(true).open(path.clone())?;
        file.write_all(data)?;
        file.sync_all()?;

        Ok(path)
    }

    impl ClientConfig {
        // NOTE: Currently used only for tests, but can be later used in
        //       other places, but then the unsupported feature should be
        //       implemented.
        /// Loads configuration from local kubeconfig file, the same
        /// one that kubectl uses.
        fn load_kube_config(resolver: Resolver) -> Option<Self> {
            let config = kube_config::load_kube_config()?.unwrap();
            // Get current context
            let context = &config
                .contexts
                .iter()
                .find(|context| context.name == config.current_context)?
                .context;
            // Get current user
            let user = &config
                .auth_infos
                .iter()
                .find(|user| user.name == context.user)?
                .auth_info;
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
            assert!(user.client_key_data.is_none(), "Not yet supported");

            let certificate_authority_path = cluster
                .certificate_authority
                .clone()
                .map(PathBuf::from)
                .or_else(|| {
                    cluster.certificate_authority_data.as_ref().map(|data| {
                        store_to_file(data.as_bytes())
                            .expect("Failed to store certificate authority public key.")
                    })
                });

            let client_certificate_path = user
                .client_certificate
                .clone()
                .map(PathBuf::from)
                .or_else(|| {
                    user.client_certificate_data.as_ref().map(|data| {
                        store_to_file(data.as_bytes()).expect("Failed to store clients public key.")
                    })
                });

            // Construction
            Some(ClientConfig {
                resolver,
                token: user.token.clone(),
                server: Uri::from_str(&cluster.server).unwrap(),
                tls_settings: TlsSettings::from_options(&Some(TlsOptions {
                    verify_certificate: cluster.insecure_skip_tls_verify,
                    ca_path: certificate_authority_path,
                    crt_path: client_certificate_path,
                    key_path: user.client_key.clone().map(PathBuf::from),
                    ..TlsOptions::default()
                }))
                .unwrap(),
            })
        }
    }

    #[test]
    fn watch_pod() {
        let namespace = format!("watch-pod-{}", Uuid::new_v4());
        let kube = Kube::new(namespace.as_str());

        let rt = runtime();

        let (sender, receiver) = channel();

        // May pickup other pods, which is fine.
        let mut client =
            ClientConfig::load_kube_config(Resolver::new(Vec::new(), rt.executor()).unwrap())
                .expect("Kubernetes configuration file not present.")
                .build_pod_watch(WatchOptional::default(), move |_| {
                    let _ = sender.send(());
                })
                .unwrap();

        rt.executor().spawn_std(async move {
            client.run().await;
        });

        // Start echo
        let _echo = echo(&kube, "echo", "210");

        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("Client didn't saw Pod change.");
    }
}
