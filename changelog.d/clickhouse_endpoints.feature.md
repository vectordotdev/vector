The `clickhouse` sink now supports multiple endpoints through the new `endpoints` configuration option. This enables high availability and load balancing across multiple ClickHouse instances. The sink will automatically distribute traffic across healthy endpoints and perform health checks to ensure reliability.

When using multiple endpoints, you can configure health checking behavior through the new `distribution` option, which allows setting retry backoff parameters for unhealthy endpoints.

authors: pinylin
