The `kubernetes_logs` source now supports end-to-end acknowledgements. When enabled, file checkpoints only advance after downstream sinks confirm event delivery, preventing data loss on source crashes or restarts.

authors: connoryy
