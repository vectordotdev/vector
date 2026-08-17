The `kubernetes_logs` source now falls back to extracting pod metadata from the log file path when the pod is not found in the Kubernetes API store. Previously, if the pod was deleted before Vector could look it up, the event was sent downstream with no kubernetes metadata at all, causing errors in downstream transforms that expect fields like `pod_namespace` to be present.

On this fallback path, Vector still populates `pod_name`, `pod_namespace`, and `container_name`. The path segment that is usually a Pod UID is exposed as `pod_log_directory_id` (not `pod_uid`), because for static pods it can be a config hash instead of the API Pod UID. Users who want UID semantics can remap the field.

authors: vparfonov
