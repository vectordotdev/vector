---
title: PostgreSQL metrics
description: Collect metrics from the [PostgreSQL](https://postgresql.org) database
kind: source
---

[PostgreSQL], also known as **Postgres**, is a powerful open source object-relational database system with over 30 years of active development. Postgres has earned its strong reputation for reliability, feature robustness, and performance.

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Context

{{< snippet "context" >}}

### Required privileges

PostgreSQL Metrics component collects metrics by making queries to the configured PostgreSQL server. Ensure the configured user is allowed to make the select queries against the following views:

* [`pg_stat_database`][pg_stat_database]
* [`pg_stat_database_conflicts`][pg_stat_database_conflicts]
* [`pg_stat_bgwriter`][pg_stat_bgwriter]

### State

{{< snippet "stateless" >}}

[pg_stat_database]: https://vector.dev/docs/reference/configuration/sources/postgresql_metrics/#pg_stat_database
[pg_stat_database_conflicts]: https://vector.dev/docs/reference/configuration/sources/postgresql_metrics/#pg_stat_database_conflicts
[pg_stat_bgwriter]: https://vector.dev/docs/reference/configuration/sources/postgresql_metrics/#pg_stat_bgwriter
[postgresql]: https://postgresql.org
