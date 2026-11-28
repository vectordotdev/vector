Introduced a `kubernetes_events` source that streams Kubernetes Event objects from the `events.k8s.io/v1` API. It supports field and label selectors, filtering by event type, reason, and involved object kind, a deduplication cache bounded by both time and size, and Lease-based leader election for replicated deployments. The elected leader persists per-stream Kubernetes resource-version checkpoints on the Lease so that a replica taking over resumes where the previous leader stopped instead of re-emitting recent events.

authors: elohmeier
