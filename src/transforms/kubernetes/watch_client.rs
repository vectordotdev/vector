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

/// Extracted metadata with list of field names and their values.
#[derive(Clone, PartialEq)]
pub struct Metadata(pub Vec<(Atom, Value)>);

impl Eq for Metadata {}

impl Metadata {
    /// Adds metadata to the event.
    pub fn enrich(&self, event: &mut LogEvent) {
        for (key, value) in self.0.iter() {
            event.insert(key.clone(), value.clone());
        }
    }
}



/// Kubernetes client which watches for T changes on Kubernetes API endpoint.
struct WatchClient<T> {
    /// Must add:
    ///  - uri
    ///  - resource_version
    ///  - watch field
    /// This can be achieved with for example `Pod::watch_pod_for_all_namespaces`.
    request_builder: Box<dyn Fn(Version)->Result<Request<Vec<u8>>,BuildError>  + Send + Sync + 'static>,
    updater: Box<dyn FnMut(T) + Send + Sync + 'static>,
    /// Box<_> is needed because WriteHandle requiers Value to implement ShallowCopy which
    /// we won't implement ourselves as it require unsafe.
    map: std::sync::Mutex<evmap::WriteHandle<Bytes, Box<Metadata>>>,
    node_name: String,
    token: Option<String>,
    host: String,
    port: String,
    client: hyper::Client<HttpsConnector<HttpConnector<Resolver>>>,
}

impl WatchClient {
    fn new(
        : &KubePodMetadata,
        map: evmap::WriteHandle<Bytes, Box<Metadata>>,
        resolver: Resolver,
        node_name: String,
        token: Option<String>,
        host: String,
        port: String,
        tls_settings: TlsSettings,
    ) -> Result<Self, BuildError> {
        // Select Pod metadata fields which are extracted and then added to Events.
        let fields = all_fields()
            .into_iter()
            .filter(|(key, _)| {
                trans_config
                    .fields
                    .iter()
                    .any(|field| field.as_str() == *key)
            })
            .map(|(_, fun)| fun)
            .collect();

        let this = Self {
            fields,
            map: std::sync::Mutex::new(map),
            node_name,
            token,
            host,
            port,
            client: https_client(resolver, tls_settings).context(HttpError)?,
        };

        // Test now if the only other source of errors passes.
        self.watch_pods_request(None)?;

        Ok(this)
    }

    /// Watches for metadata changes and propagates them to Transform.
    /// Never returns
    async fn run(&mut self) {
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
                                    match WatchPodForAllNamespacesResponse::try_from_parts(
                                        status, &unused,
                                    ) {
                                        Ok((data, used_bytes)) => {
                                            // Process watch event
                                            match self.process_event(data) {
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

    /// Processes watch event comming from Kubernetes API server.
    fn process_event(&mut self, response: WatchPodForAllNamespacesResponse) -> WatchResult {
        match response {
            WatchPodForAllNamespacesResponse::Ok(event) => {
                match event {
                    WatchEvent::Added(pod)
                    | WatchEvent::Modified(pod)
                    | WatchEvent::Bookmark(pod)
                    | WatchEvent::Deleted(pod) => {
                        // In the case of Deleted, we don't delete it's data, as there could still exist unprocessed logs from that pod.
                        // Not deleteing it will cause "memory leakage" in a sense that the data won't be used ever
                        // again after some point, but the catch is that we don't know when that point is.
                        // Also considering that, on average, an entry occupies ~232B, so to 'leak' 1MB of memory, ~4500 pods would need to be
                        // created and destroyed on the same node, which is highly unlikely.
                        //
                        // An alternative would be to delay deletions of entrys by 1min. Which is a safe guess.

                        WatchResult::New(self.update(pod))
                    }
                    WatchEvent::ErrorStatus(status) => {
                        if status.code == Some(410) {
                            // 410 Gone, restart with new list.
                            // https://kubernetes.io/docs/reference/using-api/api-concepts/#410-gone-responses
                            warn!(message = "Pod list desynced. Reseting list.", cause = ?status);
                            WatchResult::Restart
                        } else {
                            debug!("Watch event with error status: {:?}", status);
                            WatchResult::New(None)
                        }
                    }
                    WatchEvent::ErrorOther(value) => {
                        debug!(?value);
                        WatchResult::New(None)
                    }
                }
            }
            WatchPodForAllNamespacesResponse::Other(Ok(_)) => {
                debug!(message = "Received wrong object from Kubernetes API.");
                WatchResult::New(None)
            }
            WatchPodForAllNamespacesResponse::Other(Err(error)) => {
                debug!(message = "Failed parsing watch list of Pods.", ?error);
                WatchResult::Reload
            }
        }
    }

    // Builds request to watch pods.
    fn watch_pods_request(
        &self,
        resource_version: Option<Version>,
    ) -> Result<Request<Body>, BuildError> {
        // Prepare request
        let (mut request, _) = api::Pod::watch_pod_for_all_namespaces(k8s::WatchOptional {
            field_selector: Some(self.field_selector().as_str()),
            resource_version: resource_version.as_ref().map(|v| v.0.as_str()),
            ..Default::default()
        })
        .context(K8SOpenapiError)?;

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

    // Selector for current Node.
    fn field_selector(&self) -> String {
        format!("spec.nodeName={}", self.node_name)
    }

    /// Extracts metadata from pod and sets them to map.
    fn update(&mut self, pod: Pod) -> Option<Version> {
        if let Some(uid) = pod.metadata.as_ref().and_then(|md| md.uid.as_ref()) {
            let uid: Bytes = uid.as_str().into();
            let fields = self.fields(&pod);

            // Update
            let map = self
                .map
                .get_mut()
                .expect("This is the only place making access.");
            map.update(uid, Box::new(fields));
            map.refresh();
        }

        pod.metadata
            .as_ref()
            .and_then(|metadata| metadata.resource_version.clone().map(Version))
    }

    /// Returns field values for given pod.
    fn fields(&self, pod: &Pod) -> Metadata {
        Metadata(self.fields.iter().flat_map(|fun| fun(pod).0).collect())
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Http client construction errored {}.", source))]
    HttpError { source: crate::Error },
    #[snafu(display("Failed constructing request: {}.", source))]
    K8SOpenapiError { source: k8s::RequestError },
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

#[derive(Clone, Debug)]
enum WatchResult {
    /// Potentialy newer version
    New(Option<Version>),
    /// Start new request with current version.
    Reload,
    /// Start new request with None version.
    Restart,
}
