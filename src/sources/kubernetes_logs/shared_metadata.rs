//! Process-local Kubernetes watches. Log readers and checkpoints remain source-local.
//!
//! Registries hold weak references: the last subscription aborts the watcher, including
//! when a source future is cancelled during reload. Watch tasks never await log delivery.

use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, LazyLock, Mutex, Weak},
    time::Duration,
};

use futures::{Stream, StreamExt};
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use kube::{
    Api, Client, Config, Resource, ResourceExt,
    runtime::{
        WatchStreamExt,
        reflector::{
            ObjectRef,
            store::{Store, Writer},
        },
        watcher,
    },
};
use serde::de::DeserializeOwned;
use tokio::{task::JoinHandle, time::Instant};

// Never log this identity or the resolved credentials used to construct it. Debug
// formatting Config is unsuitable: secret fields can be redacted to identical strings.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct ClientIdentity([u8; 32]);

impl ClientIdentity {
    pub(super) fn new(config: &Config) -> crate::Result<Self> {
        let headers: Vec<_> = config
            .headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_bytes()))
            .collect();
        // Client certificates are loaded once by kube-client. A changed file on
        // reload must create a new watch even if the kubeconfig path is unchanged.
        let cert = if config.auth_info.client_certificate_data.is_none() {
            config
                .auth_info
                .client_certificate
                .as_ref()
                .map(std::fs::read)
                .transpose()?
        } else {
            None
        };
        let private_key = if config.auth_info.client_key_data.is_none() {
            config
                .auth_info
                .client_key
                .as_ref()
                .map(std::fs::read)
                .transpose()?
        } else {
            None
        };
        let identity = serde_json::to_vec(&serde_json::json!({
            "url": config.cluster_url.to_string(),
            "namespace": config.default_namespace,
            "roots": config.root_cert,
            "auth": config.auth_info,
            "connect": config.connect_timeout,
            "read": config.read_timeout,
            "write": config.write_timeout,
            "insecure": config.accept_invalid_certs,
            "compression": config.disable_compression,
            "proxy": config.proxy_url.as_ref().map(ToString::to_string),
            "tls_name": config.tls_server_name,
            "headers": headers,
            "cert_file": cert,
            "key_file": private_key,
        }))?;
        Ok(Self(openssl::sha::sha256(&identity)))
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct WatchKey {
    pub(super) identity: ClientIdentity,
    pub(super) fields: String,
    pub(super) labels: String,
    pub(super) use_apiserver_cache: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ViewKey {
    namespace: Option<String>,
    delay: Duration,
}

// Resource-specific registries prevent type confusion. A different API identity,
// server-side selector or LIST consistency setting always gets a separate watch.
type Registry<K> = Mutex<HashMap<WatchKey, Weak<SharedWatch<K>>>>;
static PODS: LazyLock<Registry<Pod>> = LazyLock::new(Mutex::default);
static NODES: LazyLock<Registry<Node>> = LazyLock::new(Mutex::default);
static NAMESPACES: LazyLock<Registry<Namespace>> = LazyLock::new(Mutex::default);

struct SharedWatch<K: Resource<DynamicType = ()> + Clone + 'static> {
    state: Arc<Mutex<State<K>>>,
    task: JoinHandle<()>,
}

impl<K: Resource<DynamicType = ()> + Clone + 'static> Drop for SharedWatch<K> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) struct Subscription<K: Resource<DynamicType = ()> + Clone + 'static> {
    // Keep both the upstream task and the selected store alive until the reader stops.
    _watch: Arc<SharedWatch<K>>,
    view: Arc<Mutex<View<K>>>,
}

impl<K: Resource<DynamicType = ()> + Clone + 'static> Subscription<K> {
    pub(super) fn store(&self) -> Store<K> {
        self.view
            .lock()
            .expect("metadata view mutex poisoned")
            .writer
            .as_reader()
    }
}

struct View<K: Resource<DynamicType = ()> + Clone + 'static> {
    key: ViewKey,
    writer: Writer<K>,
    // One timer per object, not one queued deletion per update. Reappearance
    // cancels a deletion, including deletion/recreation with the same name.
    pending: HashMap<ObjectRef<K>, Instant>,
}

impl<K: Resource<DynamicType = ()> + Clone + 'static> View<K> {
    fn matches(&self, object: &K) -> bool {
        self.key.namespace.as_ref().is_none_or(|namespace| {
            object.labels().get("kubernetes.io/metadata.name") == Some(namespace)
        })
    }

    fn apply(&mut self, object: K) {
        let key = ObjectRef::from_obj(&object);
        if self.matches(&object) {
            self.pending.remove(&key);
            self.writer
                .apply_watcher_event(&watcher::Event::Apply(object));
        } else {
            self.delete(&key);
        }
    }

    fn delete(&mut self, key: &ObjectRef<K>) {
        if self.writer.as_reader().get(key).is_some() {
            self.pending
                .entry(key.clone())
                .or_insert_with(|| Instant::now() + self.key.delay);
        }
    }

    fn snapshot(&mut self, objects: &HashMap<ObjectRef<K>, K>) {
        let retained = self.writer.as_reader().state();
        for object in &retained {
            let key = ObjectRef::from_obj(object.as_ref());
            if !objects.get(&key).is_some_and(|object| self.matches(object)) {
                self.delete(&key);
            }
        }
        // Store readers see either complete snapshot, never a partially applied
        // relist. Carry delayed deletions through the swap as well.
        self.writer.apply_watcher_event(&watcher::Event::Init);
        for object in retained {
            let key = ObjectRef::from_obj(object.as_ref());
            if self.pending.contains_key(&key) {
                self.writer
                    .apply_watcher_event(&watcher::Event::InitApply((*object).clone()));
            }
        }
        for object in objects
            .values()
            .filter(|object| self.matches(object))
            .cloned()
            .collect::<Vec<_>>()
        {
            self.pending.remove(&ObjectRef::from_obj(&object));
            self.writer
                .apply_watcher_event(&watcher::Event::InitApply(object));
        }
        self.writer.apply_watcher_event(&watcher::Event::InitDone);
    }

    fn expire(&mut self) {
        let now = Instant::now();
        self.pending.retain(|key, deadline| {
            if *deadline > now {
                return true;
            }
            if let Some(object) = self.writer.as_reader().get(key) {
                self.writer
                    .apply_watcher_event(&watcher::Event::Delete((*object).clone()));
            }
            false
        });
    }
}

struct State<K: Resource<DynamicType = ()> + Clone + 'static> {
    // The last complete upstream snapshot; a relist is staged atomically.
    current: HashMap<ObjectRef<K>, K>,
    initializing: Option<HashMap<ObjectRef<K>, K>>,
    views: HashMap<ViewKey, Weak<Mutex<View<K>>>>,
}

impl<K: Resource<DynamicType = ()> + Clone + 'static> Default for State<K> {
    fn default() -> Self {
        Self {
            current: HashMap::new(),
            initializing: None,
            views: HashMap::new(),
        }
    }
}

impl<K: Resource<DynamicType = ()> + Clone + 'static> State<K> {
    fn subscribe(&mut self, key: ViewKey) -> Arc<Mutex<View<K>>> {
        self.views.retain(|_, view| view.strong_count() > 0);
        if let Some(view) = self.views.get(&key).and_then(Weak::upgrade) {
            return view;
        }
        let mut view = View {
            key: key.clone(),
            writer: Writer::default(),
            pending: HashMap::new(),
        };
        // New readers must not miss already-observed objects. Do not seed them
        // with tombstones retained solely for an older reader's deletion delay.
        view.snapshot(&self.current);
        let view = Arc::new(Mutex::new(view));
        self.views.insert(key, Arc::downgrade(&view));
        view
    }

    fn for_each_view(&mut self, mut f: impl FnMut(&mut View<K>)) {
        self.views.retain(|_, weak| {
            let Some(view) = weak.upgrade() else {
                return false;
            };
            f(&mut view.lock().expect("metadata view mutex poisoned"));
            true
        });
    }

    fn event(&mut self, event: watcher::Event<K>) {
        match event {
            watcher::Event::Apply(object) => {
                self.current
                    .insert(ObjectRef::from_obj(&object), object.clone());
                self.for_each_view(|view| view.apply(object.clone()));
            }
            watcher::Event::Delete(object) => {
                let key = ObjectRef::from_obj(&object);
                // A late deletion for an old UID must not delete its replacement.
                if self
                    .current
                    .get(&key)
                    .is_some_and(|current| current.uid() != object.uid())
                {
                    return;
                }
                self.current.remove(&key);
                self.for_each_view(|view| view.delete(&key));
            }
            watcher::Event::Init => self.initializing = Some(HashMap::new()),
            watcher::Event::InitApply(object) => {
                self.initializing
                    .get_or_insert_with(HashMap::new)
                    .insert(ObjectRef::from_obj(&object), object);
            }
            watcher::Event::InitDone => {
                let next = self.initializing.take().unwrap_or_default();
                self.for_each_view(|view| view.snapshot(&next));
                self.current = next;
            }
        }
    }

    fn next_expiry(&mut self) -> Option<Instant> {
        let mut next = None;
        self.for_each_view(|view| {
            view.expire();
            if let Some(deadline) = view.pending.values().min() {
                next = Some(next.map_or(*deadline, |existing: Instant| existing.min(*deadline)));
            }
        });
        next
    }
}

async fn run<K, S>(state: Arc<Mutex<State<K>>>, stream: S)
where
    K: Resource<DynamicType = ()> + Clone + 'static,
    S: Stream<Item = watcher::Result<watcher::Event<K>>>,
{
    tokio::pin!(stream);
    loop {
        let expiry = state
            .lock()
            .expect("metadata cache mutex poisoned")
            .next_expiry();
        let timer = async {
            match expiry {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            event = stream.next() => match event {
                Some(Ok(event)) => state.lock().expect("metadata cache mutex poisoned").event(event),
                Some(Err(error)) => warn!(message = "Shared Kubernetes watch failed. Retrying.", ?error),
                None => return,
            },
            () = timer => {},
        }
    }
}

fn subscribe<K, S>(
    registry: &Registry<K>,
    key: WatchKey,
    view_key: ViewKey,
    stream: impl FnOnce() -> S,
) -> Subscription<K>
where
    K: Resource<DynamicType = ()> + Clone + Send + Sync + 'static,
    S: Stream<Item = watcher::Result<watcher::Event<K>>> + Send + 'static,
{
    let mut registry = registry.lock().expect("metadata registry mutex poisoned");
    registry.retain(|_, cache| cache.strong_count() > 0);
    let cache = registry
        .get(&key)
        .and_then(Weak::upgrade)
        .filter(|cache| !cache.task.is_finished())
        .unwrap_or_else(|| {
            let state = Arc::new(Mutex::new(State::default()));
            let task = crate::spawn_in_current_span(run(Arc::clone(&state), stream()));
            let cache = Arc::new(SharedWatch { state, task });
            registry.insert(key, Arc::downgrade(&cache));
            cache
        });
    let view = cache
        .state
        .lock()
        .expect("metadata cache mutex poisoned")
        .subscribe(view_key);
    Subscription {
        _watch: cache,
        view,
    }
}

fn watch<K>(
    client: Client,
    key: &WatchKey,
) -> impl Stream<Item = watcher::Result<watcher::Event<K>>> + Send + 'static
where
    K: Resource<DynamicType = ()> + Clone + DeserializeOwned + Debug + Send + Sync + 'static,
{
    let config = watcher::Config {
        field_selector: Some(key.fields.clone()),
        label_selector: Some(key.labels.clone()),
        list_semantic: if key.use_apiserver_cache {
            watcher::ListSemantic::Any
        } else {
            watcher::ListSemantic::MostRecent
        },
        page_size: super::get_page_size(key.use_apiserver_cache),
        ..Default::default()
    };
    watcher(Api::<K>::all(client), config).backoff(watcher::DefaultBackoff::default())
}

pub(super) fn pods(client: Client, key: WatchKey, delay: Duration) -> Subscription<Pod> {
    let stream = watch(client, &key);
    subscribe(
        &PODS,
        key,
        ViewKey {
            namespace: None,
            delay,
        },
        || stream,
    )
}

pub(super) fn nodes(client: Client, key: WatchKey, delay: Duration) -> Subscription<Node> {
    let stream = watch(client, &key);
    subscribe(
        &NODES,
        key,
        ViewKey {
            namespace: None,
            delay,
        },
        || stream,
    )
}

// The operator's common per-namespace selector can share one namespace watch.
// Other selectors are left server-side, rather than approximating Kubernetes'
// selector grammar locally. Identical selectors still share their watch/store.
fn namespace_scope(selector: &str) -> (String, Option<String>) {
    let prefix = "vector.dev/exclude!=true,kubernetes.io/metadata.name=";
    if let Some(namespace) = selector.strip_prefix(prefix) {
        let namespace = namespace.strip_prefix('=').unwrap_or(namespace);
        if !namespace.is_empty()
            && namespace.len() <= 63
            && namespace
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            && !namespace.starts_with('-')
            && !namespace.ends_with('-')
        {
            return (
                "vector.dev/exclude!=true".to_owned(),
                Some(namespace.to_owned()),
            );
        }
    }
    (selector.to_owned(), None)
}

pub(super) fn namespaces(
    client: Client,
    mut key: WatchKey,
    delay: Duration,
) -> Subscription<Namespace> {
    let (labels, namespace) = namespace_scope(&key.labels);
    key.labels = labels;
    let stream = watch(client, &key);
    subscribe(&NAMESPACES, key, ViewKey { namespace, delay }, || stream)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::channel::mpsc;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    use super::*;

    fn key() -> WatchKey {
        WatchKey {
            identity: ClientIdentity([0; 32]),
            fields: String::new(),
            labels: "vector.dev/exclude!=true".into(),
            use_apiserver_cache: false,
        }
    }

    fn view(namespace: Option<&str>, seconds: u64) -> ViewKey {
        ViewKey {
            namespace: namespace.map(str::to_owned),
            delay: Duration::from_secs(seconds),
        }
    }

    fn namespace(name: &str) -> Namespace {
        Namespace {
            metadata: ObjectMeta {
                name: Some(name.into()),
                uid: Some(format!("uid-{name}")),
                labels: Some([("kubernetes.io/metadata.name".into(), name.into())].into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn contents(view: &Arc<Mutex<View<Namespace>>>) -> Vec<String> {
        let mut names: Vec<_> = view
            .lock()
            .unwrap()
            .writer
            .as_reader()
            .state()
            .iter()
            .map(|ns| ns.name_any())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn shares_watch_and_identical_views_but_not_namespace_selection() {
        let registry = Registry::<Namespace>::default();
        let starts = AtomicUsize::new(0);
        let mut subscriptions = Vec::new();
        // Model a large operator-generated config: namespace count must not
        // become upstream watch count.
        for i in 0..1_000 {
            subscriptions.push(subscribe(
                &registry,
                key(),
                view(Some(&format!("ns-{i}")), 60),
                || {
                    starts.fetch_add(1, Ordering::SeqCst);
                    futures::stream::pending()
                },
            ));
        }
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        let duplicate = subscribe(
            &registry,
            key(),
            view(Some("ns-0"), 60),
            futures::stream::pending,
        );
        assert!(Arc::ptr_eq(&duplicate.view, &subscriptions[0].view));
        let cache = &subscriptions[0]._watch;
        let mut state = cache.state.lock().unwrap();
        state.event(watcher::Event::Apply(namespace("ns-0")));
        state.event(watcher::Event::Apply(namespace("ns-1")));
        assert_eq!(contents(&subscriptions[0].view), ["ns-0"]);
        assert_eq!(contents(&subscriptions[1].view), ["ns-1"]);
        assert!(contents(&subscriptions[2].view).is_empty());
    }

    #[tokio::test]
    async fn scopes_credentials_fields_labels_and_list_consistency() {
        let registry = Registry::<Pod>::default();
        let base = subscribe(&registry, key(), view(None, 60), futures::stream::pending);
        let mut keys = vec![key(); 4];
        keys[0].identity = ClientIdentity([1; 32]);
        keys[1].fields = "spec.nodeName=other".into();
        keys[2].labels = "app=other".into();
        keys[3].use_apiserver_cache = true;
        for key in keys {
            let other = subscribe(&registry, key, view(None, 60), futures::stream::pending);
            assert!(!Arc::ptr_eq(&base._watch, &other._watch));
        }
    }

    #[tokio::test]
    async fn identity_includes_credentials_and_tls_not_only_endpoint() {
        let mut config = Config::new("https://kubernetes.example".parse().unwrap());
        let initial = ClientIdentity::new(&config).unwrap();
        config.auth_info.token = Some("test-token-a".into());
        let first = ClientIdentity::new(&config).unwrap();
        assert!(initial != first);
        config.auth_info.token = Some("test-token-b".into());
        let second = ClientIdentity::new(&config).unwrap();
        assert!(first != second);
        config.accept_invalid_certs = true;
        assert!(second != ClientIdentity::new(&config).unwrap());
    }

    #[tokio::test]
    async fn late_subscription_is_seeded_and_relist_is_atomic() {
        let mut state = State::<Namespace>::default();
        state.event(watcher::Event::Apply(namespace("a")));
        let first = state.subscribe(view(None, 60));
        assert_eq!(contents(&first), ["a"]);
        state.event(watcher::Event::Init);
        state.event(watcher::Event::InitApply(namespace("b")));
        let late = state.subscribe(view(Some("a"), 60));
        assert_eq!(contents(&late), ["a"]);
        assert_eq!(contents(&first), ["a"]);
        state.event(watcher::Event::InitDone);
        // Missing objects survive for their configured deletion delay.
        assert_eq!(contents(&first), ["a", "b"]);
        let fresh = state.subscribe(view(None, 30));
        assert_eq!(contents(&fresh), ["b"]);
    }

    #[tokio::test(start_paused = true)]
    async fn delays_deletion_per_view_and_handles_selector_exit() {
        let mut state = State::<Namespace>::default();
        let short = state.subscribe(view(Some("a"), 10));
        let long = state.subscribe(view(Some("a"), 30));
        state.event(watcher::Event::Apply(namespace("a")));
        // A label update leaving the local selector acts like a server-side
        // selector deletion, including retaining the last matching metadata.
        let mut changed = namespace("a");
        changed.metadata.labels = None;
        state.event(watcher::Event::Apply(changed));
        assert_eq!(contents(&short), ["a"]);
        tokio::time::advance(Duration::from_secs(11)).await;
        state.next_expiry();
        assert!(contents(&short).is_empty());
        assert_eq!(contents(&long), ["a"]);
        tokio::time::advance(Duration::from_secs(20)).await;
        state.next_expiry();
        assert!(contents(&long).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn relist_removal_expires_and_reappearance_cancels_old_deletion() {
        let mut state = State::<Namespace>::default();
        let selected = state.subscribe(view(None, 10));
        state.event(watcher::Event::Apply(namespace("a")));
        state.event(watcher::Event::Apply(namespace("b")));
        state.event(watcher::Event::Init);
        state.event(watcher::Event::InitDone);
        let old = namespace("a");
        let mut replacement = old.clone();
        replacement.metadata.uid = Some("replacement".into());
        state.event(watcher::Event::Apply(replacement.clone()));
        state.event(watcher::Event::Delete(old));
        tokio::time::advance(Duration::from_secs(11)).await;
        state.next_expiry();
        assert_eq!(contents(&selected), ["a"]);
        let stored = selected
            .lock()
            .unwrap()
            .writer
            .as_reader()
            .get(&ObjectRef::from_obj(&replacement))
            .unwrap();
        assert_eq!(stored.uid(), replacement.uid());
    }

    #[tokio::test]
    async fn unread_subscriber_does_not_block_updates_or_other_subscriber_shutdown() {
        let registry = Registry::<Namespace>::default();
        let (tx, rx) = mpsc::unbounded();
        let unread = subscribe(&registry, key(), view(Some("a"), 60), || rx);
        let active = subscribe(
            &registry,
            key(),
            view(Some("b"), 60),
            futures::stream::pending,
        );
        tx.unbounded_send(Ok(watcher::Event::Apply(namespace("b"))))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while active.store().state().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let task = active._watch.task.abort_handle();
        drop(unread);
        assert!(!task.is_finished());
        tx.unbounded_send(Ok(watcher::Event::Apply(namespace("b"))))
            .unwrap();
        drop(active);
        tokio::time::timeout(Duration::from_secs(5), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Recreating a source after the final drop creates a fresh watcher.
        let fresh = subscribe(&registry, key(), view(None, 60), futures::stream::pending);
        assert!(fresh.store().state().is_empty());
        assert_eq!(registry.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn cancellation_drops_subscription_without_graceful_shutdown() {
        let registry = Registry::<Namespace>::default();
        let sub = subscribe(&registry, key(), view(None, 60), futures::stream::pending);
        let watcher = sub._watch.task.abort_handle();
        let source = tokio::spawn(async move {
            let _subscription = sub;
            std::future::pending::<()>().await;
        });
        source.abort();
        assert!(source.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !watcher.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn namespace_selector_fast_path_does_not_broaden_other_selectors() {
        for equals in ["=", "=="] {
            assert_eq!(
                namespace_scope(&format!(
                    "vector.dev/exclude!=true,kubernetes.io/metadata.name{equals}ns-a"
                )),
                ("vector.dev/exclude!=true".into(), Some("ns-a".into()))
            );
        }
        for selector in [
            "vector.dev/exclude!=true",
            "vector.dev/exclude!=true,kubernetes.io/metadata.name in (a,b)",
            "vector.dev/exclude!=true,kubernetes.io/metadata.name=a,team=x",
            "vector.dev/exclude!=true,kubernetes.io/metadata.name!=a",
            "vector.dev/exclude!=true,kubernetes.io/metadata.name=",
        ] {
            assert_eq!(namespace_scope(selector), (selector.into(), None));
        }
    }
    #[tokio::test]
    async fn thousand_sources_make_three_upstream_lists_and_watches() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        let server = MockServer::start().await;
        for (resource, kind) in [
            ("pods", "PodList"),
            ("namespaces", "NamespaceList"),
            ("nodes", "NodeList"),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/api/v1/{resource}")))
                .and(query_param("watch", "true"))
                // Hold the watch request open so the test measures concurrent
                // subscriptions rather than reconnects of completed responses.
                .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
                .with_priority(1)
                .expect(1)
                .mount(&server)
                .await;
            let items = if resource == "namespaces" {
                vec![namespace("ns-0"), namespace("ns-1")]
            } else {
                vec![]
            };
            Mock::given(method("GET"))
                .and(path(format!("/api/v1/{resource}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "apiVersion": "v1", "kind": kind,
                    "metadata": {"resourceVersion": "1"}, "items": items,
                })))
                .with_priority(2)
                .expect(1)
                .mount(&server)
                .await;
        }
        let config = Config::new(server.uri().parse().unwrap());
        let identity = ClientIdentity::new(&config).unwrap();
        let client = Client::try_from(config).unwrap();
        let mut subscriptions = Vec::new();
        for i in 0..1_000 {
            let mut pod_key = key();
            pod_key.identity = identity.clone();
            pod_key.fields = "spec.nodeName=test-node".into();
            let mut ns_key = key();
            ns_key.identity = identity.clone();
            ns_key.labels = format!("vector.dev/exclude!=true,kubernetes.io/metadata.name=ns-{i}");
            let mut node_key = key();
            node_key.identity = identity.clone();
            node_key.fields = "metadata.name=test-node".into();
            node_key.labels.clear();
            subscriptions.push((
                pods(client.clone(), pod_key, Duration::from_secs(60)),
                namespaces(client.clone(), ns_key, Duration::from_secs(60)),
                nodes(client.clone(), node_key, Duration::from_secs(60)),
            ));
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let requests = server.received_requests().await.unwrap();
                if requests.len() >= 6 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 6);
        assert_eq!(subscriptions[0].1.store().state().len(), 1);
        assert_eq!(subscriptions[1].1.store().state().len(), 1);
        assert!(subscriptions[2].1.store().state().is_empty());
        // Removing one source does not close the shared upstream requests.
        drop(subscriptions.pop());
        assert!(!subscriptions[0].0._watch.task.is_finished());
        drop(subscriptions);
    }
}
