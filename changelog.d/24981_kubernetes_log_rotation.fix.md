The `kubernetes_logs` source now preserves partial messages across kubelet log rotation and reads each container attempt’s files in order. With `oldest_first: false`, read time remains shared across container attempts.

authors: ss25sand
