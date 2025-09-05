use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vrl::value::Value;

/// Thread-safe cache for Kubernetes metadata with composite-key indexing.
/// - Key Format: Composite keys (e.g., `pod_uid|container_name`) to prevent collisions
#[derive(Clone, Default, Debug)]
pub struct K8sMetadataCache {
    cache: Arc<Mutex<HashMap<String, Arc<Value>>>>,
}

impl K8sMetadataCache {
    /// Generates a composite cache key from Pod UID and container name.
    ///
    /// ## Format
    /// `{pod_uuid}|{container_name}`
    /// (e.g., `"c5b4a3d2|nginx"`)
    ///
    /// ## Why Composite Keys?
    /// - Uniquely identifies containers within a Pod
    /// - Avoids collisions between Pods with same container names
    fn generate_key(pod_uuid: &str, container_name: &str) -> String {
        format!("{pod_uuid}|{container_name}")
    }

    /// Retrieves cached metadata by Pod UID and container name.
    pub fn get(&self, pod_uuid: &str, container_name: &str) -> Option<Arc<Value>> {
        let key = Self::generate_key(pod_uuid, container_name);
        let cache = self.cache.lock().unwrap();
        cache.get(&key).map(Arc::clone)
    }

    /// Inserts metadata into cache with automatic type erasure.
    ///
    /// ## Type Requirements
    /// - `T: Any` → Enables runtime type checking
    /// - `T: Send + Sync` → Permits cross-thread sharing
    /// - `'static` → Guarantees no short-lived references
    ///
    /// ## Operation
    /// 1. Converts `value` into `Arc<T>`
    /// 2. Type-erases to `Arc<dyn Any + Send + Sync>`
    /// 3. Locks cache briefly for insertion
    pub fn insert(
        &self,
        pod_uuid: &str,
        container_name: &str,
        value: Value,
    ) {
        let key = Self::generate_key(pod_uuid, container_name);
        let value = Arc::new(value);
        let mut cache = self.cache.lock().unwrap();
        cache.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::K8sMetadataCache;
    use std::sync::Arc;

    #[test]
    fn test_insert_and_get_metadata() {
        let pod_uid = "pod-123";
        let container_name = "nginx";
        let cache = K8sMetadataCache::default();
        let map = btreemap! {
            "image" => "nginx:1.25",
            "ports" => vec![80, 443],
        };
        cache.insert(
            pod_uid.to_string(),
            container_name.to_string(),
            map.into(),
        );

        let result = cache.get(pod_uid, container_name);
        let cached_value = result.unwrap();
        let spec = cached_value.as_object().unwrap();
        assert_eq!(*spec.get("image").unwrap(), "nginx:1.25".into());

        let actual_ports = spec.get("ports").unwrap().as_array().unwrap().iter().map(|v| v.as_integer().unwrap()).collect::<Vec<i64>>();
        assert_eq!(actual_ports, vec![80, 443]);

        let result2 = cache.get(pod_uid, container_name).unwrap();
        assert!(Arc::ptr_eq(&cached_value, &result2));
    }
}
