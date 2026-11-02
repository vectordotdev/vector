---
what: "`metadata_refresh_failed_total` metric"
deprecated_since: "0.62.0"
---

The `metadata_refresh_failed_total` counter emitted by the `aws_ec2_metadata` transform is
deprecated. Use the `component_errors_total` metric instead.

Migrate any dashboards or alerts that reference:

- `metadata_refresh_failed_total`

to:

- `component_errors_total{component_type="aws_ec2_metadata", error_type="request_failed", stage="processing"}`

Keep these selectors, and any existing `component_id` selector, so that the query does not count errors
from unrelated components.
