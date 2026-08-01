The `kubernetes_logs` source now falls back to extracting pod metadata (namespace, pod name, pod UID, container name) from the log file path when the pod is not found in the Kubernetes API store. Previously, if the pod was deleted before Vector could look it up, the event was sent downstream with no kubernetes metadata at all, causing errors in downstream transforms that expect fields like `namespace_name` to be present.

authors: vparfonov
