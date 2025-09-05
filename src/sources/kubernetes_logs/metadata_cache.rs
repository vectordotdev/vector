use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vrl::value::Value;

/// Thread-safe cache for Kubernetes metadata with composite-key indexing.
/// - Key Format: Composite keys (e.g., `pod_uid|container_name`) to prevent collisions
pub struct K8sMetadataCache {
    cache: Arc<Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>>,
}

impl K8sMetadataCache {
    pub fn new() -> Self {
        K8sMetadataCache {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generates a composite cache key from Pod UID and container name.
    ///
    /// ## Format
    /// `{pod_uuid}|{container_name}`
    /// (e.g., `"c5b4a3d2|nginx"`)
    ///
    /// ## Why Composite Keys?
    /// - Uniquely identifies containers within a Pod
    /// - Avoids collisions between Pods with same container names
    pub fn generate_key(pod_uuid: &str, container_name: &str) -> String {
        format!("{pod_uuid}|{container_name}")
    }

    /// Retrieves cached metadata by Pod UID and container name.
    pub fn get(&self, pod_uuid: &str, container_name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
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
    pub fn insert<T: Any + Send + Sync + 'static>(
        &self,
        pod_uuid: String,
        container_name: String,
        value: T,
    ) {
        let key = Self::generate_key(&pod_uuid, &container_name);
        let value = Arc::new(value);
        let mut cache = self.cache.lock().unwrap();
        cache.insert(key, value);
    }
}

/// Converts into [`Value`] handling downcast failures via [`Value::Null`]
pub fn any_to_value(any: Arc<dyn Any + Send + Sync>) -> Value {
    if let Some(arc_val) = any.downcast_ref::<Arc<Value>>() {
        return (**arc_val).clone();
    }

    if let Some(val) = any.downcast_ref::<Value>() {
        return val.clone();
    }
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::K8sMetadataCache;
    use std::sync::Arc;

    #[test]
    fn test_insert_and_get_metadata() {
        #[derive(Debug, PartialEq)]
        struct ContainerSpec {
            image: String,
            ports: Vec<i32>,
        }

        let pod_uid = "pod-123";
        let container_name = "nginx";
        let cache = K8sMetadataCache::new();
        cache.insert(
            pod_uid.to_string(),
            container_name.to_string(),
            ContainerSpec {
                image: "nginx:1.25".into(),
                ports: vec![80, 443],
            },
        );

        let result = cache.get(pod_uid, container_name);
        assert!(result.is_some(), "Expected cache hit but got None");

        let arc_any = result.unwrap();

        let spec = arc_any.downcast_ref::<ContainerSpec>();
        assert!(spec.is_some(), "Failed to downcast to ContainerSpec");

        let spec = spec.unwrap();
        assert_eq!(spec.image, "nginx:1.25");
        assert_eq!(spec.ports, vec![80, 443]);

        let result2 = cache.get(pod_uid, container_name).unwrap();
        assert!(Arc::ptr_eq(&arc_any, &result2));
    }
}
