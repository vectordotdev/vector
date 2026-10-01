The `kubernetes_logs` source now shares compatible Kubernetes metadata watches within a
Vector process, reducing duplicate API requests when multiple sources use the same
watch settings. Log readers and checkpoints remain separate. No new configuration
options are required.

authors: anisimov-es
