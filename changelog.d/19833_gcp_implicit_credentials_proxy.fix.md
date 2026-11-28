GCP components now respect Vector's global and component-level `proxy` settings when fetching credentials from the Compute Engine metadata service.
If outbound traffic uses `proxy.http` and the metadata service must be reached directly, add `metadata.google.internal` to `proxy.no_proxy`.

authors: Captain1653
