The `prometheus_exporter` sink now supports Kubernetes SubjectAccessReview (SAR)
authentication. Incoming scrape requests are validated by verifying Bearer tokens
via TokenReview and checking permissions via SubjectAccessReview, using either
nonResourceURL or resource-based authorization. The full TokenReview identity
(username, groups, UID, extra) is forwarded to the SAR so webhook authorizers
receive complete user context.

authors: jcantrill
