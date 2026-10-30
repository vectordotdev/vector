Fixed a memory leak in internal metrics expiration. Expired metrics were removed from the registry but their keys were kept in a timeout cache forever, so memory grew with every unique set of labels seen. This mostly affected `kubernetes_logs`, where each new pod creates new `pod_name` labeled metrics.

authors: vinzee
