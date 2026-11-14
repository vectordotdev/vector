The `kubernetes_logs` source now annotates logs from init containers, sidecar containers (init containers with `restartPolicy: Always`) included, with `container_id`, `container_image` and `container_image_id`, as it already did for regular containers.

authors: astorath
