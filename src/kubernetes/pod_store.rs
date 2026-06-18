//! A UID-keyed store of pod metadata for the `kubernetes_logs` source.
//!
//! kube's reflector `Store` is keyed by name and namespace and therefore
//! mirrors only the *current* state of the cluster: when a pod is deleted and
//! another is created reusing the same name and namespace, the store can only
//! hold the newer one. Vector, however, tails log files that outlive their pod
//! (the kubelet retains them after deletion), and those files are identified on
//! disk by the pod UID. To annotate such files with the metadata of the exact
//! incarnation that produced them, we keep our own store keyed by the
//! identifier that appears in the log path.
//!
//! See <https://github.com/vectordotdev/vector/issues/13467>.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::Duration,
};

use futures::StreamExt;
use futures_util::Stream;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use kube::runtime::watcher;
use tokio::pin;
use tokio_util::time::DelayQueue;

use super::pod_manager_logic::extract_static_pod_config_hashsum;

/// Returns the identifier used to locate a pod's logs on disk: the static pod
/// config hashsum for mirror pods, otherwise the pod UID.
///
/// This is the same value used as the `uid` component of the pod log directory
/// (see `build_pod_logs_directory`), so files parsed from disk can be looked up
/// against a [`PodStore`].
pub fn pod_uid_for_path(metadata: &ObjectMeta) -> Option<String> {
    extract_static_pod_config_hashsum(metadata)
        .or(metadata.uid.as_deref())
        .map(ToOwned::to_owned)
}

/// A cloneable handle to a UID-keyed store of pods.
#[derive(Clone, Default)]
pub struct PodStore {
    pods: Arc<RwLock<HashMap<String, Arc<Pod>>>>,
}

impl PodStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the pod tracked under the given path identifier, if any.
    pub fn get(&self, uid: &str) -> Option<Arc<Pod>> {
        self.pods.read().unwrap().get(uid).cloned()
    }

    /// Returns all currently tracked pods.
    pub fn list(&self) -> Vec<Arc<Pod>> {
        self.pods.read().unwrap().values().cloned().collect()
    }

    fn insert(&self, pod: Pod) {
        if let Some(uid) = pod_uid_for_path(&pod.metadata) {
            self.pods.write().unwrap().insert(uid, Arc::new(pod));
        }
    }

    fn remove(&self, uid: &str) {
        self.pods.write().unwrap().remove(uid);
    }

    fn uids(&self) -> HashSet<String> {
        self.pods.read().unwrap().keys().cloned().collect()
    }
}

/// Maintains a [`PodStore`] from a watcher stream, delaying deletions by
/// `delay_deletion` so that logs from a deleted pod can still be annotated
/// while its files are being drained.
///
/// Because the store is keyed by the pod's path identifier (UID), a pod that is
/// deleted and recreated reusing the same name and namespace occupies a
/// distinct key. The delayed deletion of the old incarnation therefore can
/// never evict the new one. See
/// <https://github.com/vectordotdev/vector/issues/12014>.
pub async fn pod_reflector<W>(store: PodStore, stream: W, delay_deletion: Duration)
where
    W: Stream<Item = watcher::Result<watcher::Event<Pod>>>,
{
    pin!(stream);
    let mut delay_queue: DelayQueue<String> = DelayQueue::default();
    // UIDs currently believed to be alive. Used to avoid removing a pod whose
    // deletion was scheduled but which was re-applied before the delay elapsed.
    let mut live: HashSet<String> = HashSet::new();
    let mut init_buffer: Vec<Pod> = Vec::new();
    loop {
        tokio::select! {
            result = stream.next() => {
                match result {
                    Some(Ok(event)) => match event {
                        // Immediately reconcile `Apply` events.
                        watcher::Event::Apply(pod) => {
                            if let Some(uid) = pod_uid_for_path(&pod.metadata) {
                                live.insert(uid);
                            }
                            store.insert(pod);
                        }
                        // Delay reconciling `Delete` events.
                        watcher::Event::Delete(pod) => {
                            if let Some(uid) = pod_uid_for_path(&pod.metadata) {
                                live.remove(&uid);
                                delay_queue.insert(uid, delay_deletion);
                            }
                        }
                        // Begin buffering a relist.
                        watcher::Event::Init => {
                            init_buffer.clear();
                        }
                        watcher::Event::InitApply(pod) => {
                            init_buffer.push(pod);
                        }
                        // Reconcile the relist: apply everything observed, and
                        // delay the deletion of pods that are no longer present.
                        watcher::Event::InitDone => {
                            let new_uids: HashSet<String> = init_buffer
                                .iter()
                                .filter_map(|pod| pod_uid_for_path(&pod.metadata))
                                .collect();
                            for uid in store.uids() {
                                if !new_uids.contains(&uid) && live.remove(&uid) {
                                    delay_queue.insert(uid, delay_deletion);
                                }
                            }
                            for pod in init_buffer.drain(..) {
                                if let Some(uid) = pod_uid_for_path(&pod.metadata) {
                                    live.insert(uid);
                                }
                                store.insert(pod);
                            }
                        }
                    },
                    Some(Err(error)) => {
                        warn!(message = "Watcher stream received an error. Retrying.", ?error);
                    }
                    // The watcher stream should never yield `None`.
                    None => unreachable!("a watcher Stream never ends"),
                }
            }
            result = delay_queue.next(), if !delay_queue.is_empty() => {
                match result {
                    Some(expired) => {
                        let uid = expired.into_inner();
                        // Skip removal if the pod was re-applied during the delay.
                        if !live.contains(&uid) {
                            store.remove(&uid);
                        }
                    }
                    // DelayQueue returns None only when exhausted, but the
                    // branch is disabled while the queue is empty.
                    None => unreachable!("an empty DelayQueue is never polled"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use futures::channel::mpsc;
    use futures_util::SinkExt;
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use kube::runtime::watcher;

    use super::{PodStore, pod_reflector, pod_uid_for_path};

    fn pod(name: &str, uid: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some("ns".to_string()),
                uid: Some(uid.to_string()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    #[test]
    fn pod_uid_for_path_prefers_static_hashsum() {
        let mut p = pod("foo", "real-uid");
        assert_eq!(pod_uid_for_path(&p.metadata).as_deref(), Some("real-uid"));

        let mut annotations = BTreeMap::new();
        annotations.insert(
            "kubernetes.io/config.mirror".to_string(),
            "hash123".to_string(),
        );
        p.metadata.annotations = Some(annotations);
        assert_eq!(pod_uid_for_path(&p.metadata).as_deref(), Some("hash123"));
    }

    #[tokio::test]
    async fn recreated_pod_keeps_both_incarnations_during_delay() {
        // A pod is deleted and recreated reusing the same name/namespace with a
        // new UID. Both incarnations must be addressable during the delay so
        // that the old incarnation's still-draining files annotate correctly,
        // and the new incarnation is never evicted.
        let store = PodStore::new();
        let (mut tx, rx) = mpsc::channel(5);
        tx.send(Ok(watcher::Event::Apply(pod("foo-0", "uid-a"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(pod("foo-0", "uid-a"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(pod("foo-0", "uid-b"))))
            .await
            .unwrap();
        tokio::spawn(pod_reflector(store.clone(), rx, Duration::from_secs(2)));

        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(store.get("uid-a").is_some());
        assert!(store.get("uid-b").is_some());

        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(store.get("uid-a").is_none());
        assert!(store.get("uid-b").is_some());
    }

    #[tokio::test]
    async fn reapplied_pod_survives_pending_deletion() {
        let store = PodStore::new();
        let (mut tx, rx) = mpsc::channel(5);
        tx.send(Ok(watcher::Event::Apply(pod("foo", "uid-a"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Delete(pod("foo", "uid-a"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(pod("foo", "uid-a"))))
            .await
            .unwrap();
        tokio::spawn(pod_reflector(store.clone(), rx, Duration::from_secs(2)));

        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(store.get("uid-a").is_some());
    }

    #[tokio::test]
    async fn relist_retains_absent_pod_during_delay() {
        let store = PodStore::new();
        let (mut tx, rx) = mpsc::channel(8);
        tx.send(Ok(watcher::Event::Apply(pod("a", "uid-a"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::Apply(pod("b", "uid-b"))))
            .await
            .unwrap();
        // A relist that no longer reports pod `a`.
        tx.send(Ok(watcher::Event::Init)).await.unwrap();
        tx.send(Ok(watcher::Event::InitApply(pod("b", "uid-b"))))
            .await
            .unwrap();
        tx.send(Ok(watcher::Event::InitDone)).await.unwrap();
        tokio::spawn(pod_reflector(store.clone(), rx, Duration::from_secs(2)));

        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(store.get("uid-a").is_some());
        assert!(store.get("uid-b").is_some());

        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(store.get("uid-a").is_none());
        assert!(store.get("uid-b").is_some());
    }
}
