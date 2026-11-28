---
what: "`aggregate_failed_updates` counter without the `_total` suffix"
deprecated_since: "0.59.0"
---

The `aggregate` transform now also emits the failed-updates counter with the `_total` suffix, matching Vector's counter naming convention:

- `aggregate_failed_updates_total`

The previous name without the `_total` suffix is deprecated and will be removed in a future release. Migrate any dashboards or alerts that reference:

- `aggregate_failed_updates`
