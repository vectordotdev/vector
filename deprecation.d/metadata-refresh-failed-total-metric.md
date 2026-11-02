---
what: "`metadata_refresh_failed_total` metric"
deprecated_since: "0.62.0"
---

The `metadata_refresh_failed_total` counter emitted by the `aws_ec2_metadata` transform is
deprecated. Use the `component_errors_total` metric instead.

Migrate any dashboards or alerts that reference:

- `metadata_refresh_failed_total`

to:

- `component_errors_total`
