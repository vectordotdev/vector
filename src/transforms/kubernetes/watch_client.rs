use crate::{
    dns::Resolver,
    sinks::util::http::HttpClient,
    tls::{self, TlsOptions, TlsSettings},
};
use bytes::Bytes;
use futures01::{future::Future, stream::Stream};
use http::{header, status::StatusCode, uri, Request, Uri};
use k8s_openapi::{
    api::core::v1::{Pod, WatchPodForAllNamespacesResponse},
    apimachinery::pkg::apis::meta::v1::WatchEvent,
    RequestError, Response, ResponseError, WatchOptional,
};
use snafu::{futures01::future::FutureExt, ResultExt, Snafu};
use std::{fs, io};
use tower::Service;

// ************************ Defined by Kubernetes *********************** //
// API access is defined with
// https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster/#accessing-the-api-from-a-pod

/// File in which Kubernetes stores service account token.
const TOKEN_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";

/// Kuberentes API should be reachable at this address
const KUBERNETES_SERVICE_ADDRESS: &str = "https://kubernetes.default.svc";

/// Path to certificate authority certificate
const CA_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";

/// Config which could be loaded from kubeconfig or local kubernetes cluster.
/// Used to build WatchClient which is in turn used to build Stream of metadata.
#[derive(Clone)]
pub struct ClientConfig {
    resolver: Resolver,
    token: Option<String>,
    server: Uri,
    tls_settings: TlsSettings,
    node_name: Option<String>,
}

impl ClientConfig {
    /// Loads Kubernetes API access information available to Pods of cluster.
    pub fn in_cluster(node: String, resolver: Resolver) -> Result<Self, BuildError> {
        let server = Uri::from_static(KUBERNETES_SERVICE_ADDRESS);

        let token = fs::read(TOKEN_PATH)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    BuildError::MissingAccountToken
                } else {
                    BuildError::FailedReadingAccountToken { source: error }
                }
            })
            .and_then(|bytes| String::from_utf8(bytes).context(AccountTokenCorrupted))?;

        let mut options = TlsOptions::default();
        options.ca_path = Some(CA_PATH.into());
        let tls_settings = TlsSettings::from_options(&Some(options)).context(TlsError)?;

        Ok(Self {
            resolver,
            token: Some(token),
            server,
            tls_settings,
            node_name: Some(node),
        })
    }

    pub fn build(&self) -> Result<WatchClient, BuildError> {
        let client =
            HttpClient::new(self.resolver.clone(), self.tls_settings.clone()).context(HttpError)?;

        Ok(WatchClient {
            client,
            config: self.clone(),
        })
    }

    // Builds request to watch Pod data.
    fn build_request(&self, version: Option<Version>) -> Result<Request<hyper::Body>, BuildError> {
        // Selector for current node
        let field_selector = self
            .node_name
            .as_ref()
            .map(|node_name| format!("spec.nodeName={}", node_name));

        let (mut request, _) = Pod::watch_pod_for_all_namespaces(WatchOptional {
            resource_version: version.as_ref().map(|v| v.0.as_str()),
            field_selector: field_selector.as_ref().map(|selector| selector.as_str()),
            ..WatchOptional::default()
        })
        .context(K8SOpenapiError)?;

        // Authorize
        if let Some(token) = self.token.as_ref() {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(format!("Bearer {}", token).as_str())
                    .context(InvalidToken)?,
            );
        }

        // Fill uri
        let mut uri = self.server.clone().into_parts();
        uri.path_and_query = request.uri().clone().into_parts().path_and_query;
        *request.uri_mut() = Uri::from_parts(uri).context(InvalidUriParts)?;

        let (parts, body) = request.into_parts();
        Ok(Request::from_parts(parts, body.into()))
    }
}

/// Kubernetes client for watching changes on Kubernetes Objects.
pub struct WatchClient {
    config: ClientConfig,
    client: HttpClient,
}

impl WatchClient {
    /// Builds Stream of newest Pod metadata.
    /// With version None, will also stream inital Pod metadata.
    ///
    /// Caller should maintain latest `pod.metadata.resource_version` and must stop
    /// using stream on first RuntimeError and start a new one.
    ///
    /// When encountering end of stream, caller should use RuntimeError::WatchUnexpectedlyEnded.
    ///
    /// Arguments:
    ///  - `from` should contain latest `pod.metadata.resource_version`.
    ///  - `error` must contain RuntimeError with which last stream ended.
    pub fn watch_metadata(
        &mut self,
        mut version: Option<Version>,
        error: Option<RuntimeError>,
    ) -> Result<impl Stream<Item = (Pod, PodEvent), Error = RuntimeError>, BuildError> {
        match error {
            None => (),
            Some(RuntimeError::WatchEventError { status }) if status.code == Some(410) => {
                // 410 Gone, restart with new list.
                // https://kubernetes.io/docs/reference/using-api/api-concepts/#410-gone-responses
                info!("Watch list desynced for Kubernetes Pod list. Restarting version.");
                version = None;
            }
            Some(error) => debug!(%error),
        }

        // If watch is initiated with None resource_version, we will receive initial
        // list of data as synthetic "Added" events.
        // https://kubernetes.io/docs/reference/using-api/api-concepts/#resource-versions
        let request = self.config.build_request(version)?;

        Ok(self.build_watch_stream(request))
    }

    fn build_watch_stream(
        &mut self,
        request: Request<hyper::Body>,
    ) -> impl Stream<Item = (Pod, PodEvent), Error = RuntimeError> {
        let mut decoder = Decoder::default();

        self.client
            .call(request)
            .context(FailedConnecting)
            .and_then(|response| {
                let status = response.status();
                if status == StatusCode::OK {
                    // Connected succesfully
                    info!(message = "Watching for changes.");

                    Ok(response
                        .into_body()
                        .map_err(|error| RuntimeError::WatchConnectionErrored { source: error }))
                } else {
                    Err(RuntimeError::ConnectionStatusNotOK { status })
                }
            })
            .flatten_stream()
            // Process Server responses/data.
            .map(move |chunk| decoder.decode(chunk.into_bytes()))
            .flatten()
            // Extracts event from response
            .and_then(|response| match response {
                WatchPodForAllNamespacesResponse::Ok(event) => Ok(event),
                WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                    Err(RuntimeError::WrongObjectInResponse)
                }
                WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                    Err(error).context(ResponseParseError)
                }
            })
            // Extracts Pod metadata from event
            .and_then(|event| match event {
                WatchEvent::Added(data)
                | WatchEvent::Modified(data)
                | WatchEvent::Bookmark(data) => Ok((data, PodEvent::Changed)),
                WatchEvent::Deleted(data) => Ok((data, PodEvent::Deleted)),
                WatchEvent::ErrorStatus(status) => Err(RuntimeError::WatchEventError { status }),
                WatchEvent::ErrorOther(other) => {
                    Err(RuntimeError::UnknownWatchEventError { other })
                }
            })
    }
}

/// Decodes responses from incoming Chunks
#[derive(Debug, Default)]
struct Decoder {
    // Unused bytes from Server responses.
    unused: Vec<u8>,
}

impl Decoder {
    fn decode(
        &mut self,
        chunk: Bytes,
    ) -> impl Stream<Item = WatchPodForAllNamespacesResponse, Error = RuntimeError> {
        // We need to process unused data as soon as we get
        // them. Because a watch on Kubernetes object behaves
        // like a never ending stream of bytes.

        // Append new data to unused.
        self.unused.extend_from_slice(&chunk[..]);

        // Decodes watch response from unused data.
        // Repeats decoding so long as there is sufficient data.
        // Removes used data.
        let mut decoded = Vec::new();
        loop {
            match WatchPodForAllNamespacesResponse::try_from_parts(StatusCode::OK, &self.unused) {
                Ok((response, used_bytes)) => {
                    assert!(used_bytes > 0, "Parser must consume some data");
                    // Remove used data.
                    let _ = self.unused.drain(..used_bytes);

                    decoded.push(Ok(response));
                    // Continue decoding out data.
                }
                Err(ResponseError::NeedMoreData) => break,
                Err(error) => {
                    decoded.push(Err(RuntimeError::ParseResponseError {
                        name: "WatchPodForAllNamespacesResponse".to_owned(),
                        error,
                    }));
                    break;
                }
            };
        }

        // Returns all currently decodable watch responses.
        futures01::stream::iter_result(decoded)
    }
}

/// Event that changed the metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PodEvent {
    Changed,
    Deleted,
}

/// Version of Kubernetes resource
#[derive(Clone, Debug)]
pub struct Version(String);

impl Version {
    pub fn from_pod(pod: &Pod) -> Option<Self> {
        pod.metadata
            .as_ref()
            .and_then(|metadata| metadata.resource_version.clone().map(Version))
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
    #[snafu(display("Missing Kubernetes service account token file. Probably because Vector isn't in Kubernetes Pod."))]
    MissingAccountToken,
    #[snafu(display("Failed reading Kubernetes service account token: {}.", source))]
    FailedReadingAccountToken { source: io::Error },
    #[snafu(display("Kubernetes service account token is corrupted: {}.", source))]
    AccountTokenCorrupted { source: std::string::FromUtf8Error },
    #[snafu(display("TLS construction errored {}.", source))]
    TlsError { source: tls::TlsError },
}

#[derive(Debug, Snafu)]
pub enum RuntimeError {
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
    use crate::{dns::Resolver, tls::TlsSettings};
    use http::Uri;

    #[test]
    fn buildable() {
        let rt = crate::runtime::Runtime::new().unwrap();
        let _ = ClientConfig {
            resolver: Resolver::new(Vec::new(), rt.executor()).unwrap(),
            token: None,
            server: Uri::from_static("https://localhost:8001"),
            tls_settings: TlsSettings::from_options(&None).unwrap(),
            node_name: None,
        }
        .build()
        .unwrap()
        .watch_metadata(None, None)
        .unwrap();
    }
}

#[cfg(test)]
mod kube_tests {
    #![cfg(feature = "kubernetes-integration-tests")]

    use super::ClientConfig;
    use crate::{
        dns::Resolver,
        sources::kubernetes::test::{echo, Kube},
        test_util::{runtime, temp_file},
        tls::{TlsOptions, TlsSettings},
    };
    use dirs;
    use futures01::{future::Future, stream::Stream};
    use http::Uri;
    use kube::config::Config;
    use serde_yaml;
    use snafu::{ResultExt, Snafu};
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        path::PathBuf,
        str::FromStr,
        sync::mpsc::channel,
        time::Duration,
    };
    use uuid::Uuid;

    /// Enviorment variable that can containa path to kubernetes config file.
    const CONFIG_PATH: &str = "KUBECONFIG";

    fn store_to_file(data: &[u8]) -> Result<PathBuf, std::io::Error> {
        let path = temp_file();

        let mut file = OpenOptions::new().write(true).open(path.clone())?;
        file.write_all(data)?;
        file.sync_all()?;

        Ok(path)
    }

    /// Loads configuration from local kubeconfig file, the same
    /// one that kubectl uses.
    /// None if such file doesn't exist.
    fn load_kube_config() -> Option<Result<Config, KubeConfigLoadError>> {
        let path = std::env::var(CONFIG_PATH)
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".kube").join("config")))?;

        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                return Some(Err(KubeConfigLoadError::FileError { source: error }));
            }
        };

        Some(serde_yaml::from_reader(file).context(ParsingError))
    }

    #[derive(Debug, Snafu)]
    pub enum KubeConfigLoadError {
        #[snafu(display("Error opening Kubernetes config file: {}.", source))]
        FileError { source: std::io::Error },
        #[snafu(display("Error parsing Kubernetes config file: {}.", source))]
        ParsingError { source: serde_yaml::Error },
    }

    impl ClientConfig {
        // NOTE: Currently used only for tests, but can be later used in
        //       other places, but then the unsupported feature should be
        //       implemented.
        //
        /// Loads configuration from local kubeconfig file, the same
        /// one that kubectl uses.
        fn load_kube_config(resolver: Resolver) -> Option<Self> {
            let config = load_kube_config()?.unwrap();
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
                node_name: None,
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
    #[ignore]
    fn watch_pod() {
        let namespace = format!("watch-pod-{}", Uuid::new_v4());
        let kube = Kube::new(namespace.as_str());

        let mut rt = runtime();

        let (sender, receiver) = channel();

        // May pickup other pods, which is fine.
        let mut client =
            ClientConfig::load_kube_config(Resolver::new(Vec::new(), rt.executor()).unwrap())
                .expect("Kubernetes configuration file not present.")
                .build()
                .unwrap();

        let stream = client.watch_metadata(None, None).unwrap();

        rt.spawn(
            stream
                .map(move |_| sender.send(()))
                .into_future()
                .map(|_| ())
                .map_err(|(error, _)| error!(?error)),
        );

        // Start echo
        let _echo = echo(&kube, "echo", "210");

        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("Client did not see a Pod change.");
    }
}
