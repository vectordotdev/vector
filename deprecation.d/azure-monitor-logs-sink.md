---
what: "`azure_monitor_logs` sink"
announcement_version: 0.54.0
deprecation_version: 0.58.0
---

The `azure_monitor_logs` sink is deprecated in favor of the new `azure_logs_ingestion` sink,
which uses the Azure Monitor Logs Ingestion API.

Users should migrate before Microsoft ends support for the old Data Collector API (scheduled
for September 2026).
