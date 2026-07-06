Introduced a `kubernetes_events` source that streams Kubernetes Event objects through the API, with optional deduplication, enrichment helpers, and Lease-based leader election for replicated deployments. The elected leader persists a delivery watermark on the Lease so that a replica taking over resumes where the previous leader stopped instead of re-emitting recent events.

authors: elohmeier
