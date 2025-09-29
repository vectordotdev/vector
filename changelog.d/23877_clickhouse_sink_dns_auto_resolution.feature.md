The ClickHouse sink now supports DNS auto-resolution for load balancing, allowing automatic discovery and rotation of ClickHouse cluster nodes through DNS lookups. This enables better high availability and load distribution when connecting to ClickHouse clusters with multiple endpoints. The feature can be enabled by setting `auto_resolve_dns: true` in the ClickHouse sink configuration.

authors: sebinsunny
