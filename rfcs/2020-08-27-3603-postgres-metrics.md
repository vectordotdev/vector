# RFC 3603 - 2020-08-27 - Collecting metrics from PostgreSQL

This RFC is to introduce a new metrics source to consume metrics from PostgreSQL database servers. The high level plan is to implement one source that collects metrics from PostgreSQL server instances.

Background reading on PostgreSQL monitoring:

- https://www.datadoghq.com/blog/postgresql-monitoring/

## Scope

This RFC will cover:

- A new source for PostgreSQL server metrics.

This RFC will not cover:

- Other databases.

## Motivation

Users want to collect, transform, and forward metrics to better observe how their PostgreSQL databases are performing.

## Internal Proposal

Build a single source called `postgresql_metrics` (name to be confirmed) to collect PostgreSQL metrics. We support all non-EOL'ed PostgreSQL versions.

The recommended implementation is to use the Rust PostgreSQL client to connect the target database server by address specified in configuration.

- https://docs.rs/postgres/0.17.5/postgres/index.html


The source would then run the following queries:

- `SELECT * FROM pg_stat_database`
- `SELECT * FROM pg_stat_database_conflicts`
- `SELECT * FROM pg_stat_bgwriter`

And return these metrics by parsing the query results and converting them into metrics using the database name and column names.

- `pg_up` -> Used as an uptime metric with 0 for successful collection and 1 for failed collection (gauge)
- `pg_stat_database_blk_read_time_seconds_total` tagged with db, host, server, user (counter)
- `pg_stat_database_blk_write_time_seconds_total` tagged with db, host, server, user (counter)
- `pg_stat_database_blks_hit_total` tagged with db, host, server, user (counter)
- `pg_stat_database_blks_read_total` tagged with db, host, server, user (counter)
- `pg_stat_database_stats_reset` tagged with host, server, user (gauge)
- `pg_stat_bgwriter_buffers_alloc_total` tagged with host, server (counter)
- `pg_stat_bgwriter_buffers_backend_total` tagged with host, server (counter)
- `pg_stat_bgwriter_buffers_backend_fsync_total` tagged with host, server (counter)
- `pg_stat_bgwriter_buffers_checkpoint_total` tagged with host, server (counter)
- `pg_stat_bgwriter_buffers_clean_total` tagged with host, server (counter)
- `pg_stat_bgwriter_checkpoint_sync_time_seconds_total` tagged with host, server (counter)
- `pg_stat_bgwriter_checkpoint_write_time_seconds_total` tagged with host, server (counter)
- `pg_stat_bgwriter_checkpoints_req_total` tagged with host, server (counter)
- `pg_stat_bgwriter_checkpoints_timed_total` tagged with host, server (counter)
- `pg_stat_bgwriter_maxwritten_clean_total` tagged with host, server (counter)
- `pg_stat_bgwriter_stats_reset` host, server (gauge)
- `pg_stat_database_conflicts_total` tagged with db, host, server, user (counter)
- `pg_stat_database_datid` (database ID) tagged with db, host, server, user (gauge)
- `pg_stat_database_deadlocks_total` tagged with db, host, server, user (counter)
- `pg_stat_database_numbackends_total` tagged with db, host, server, user (gauge)
- `pg_stat_database_temp_bytes_total` tagged with db, host, server, user (counter)
- `pg_stat_database_temp_files_total` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_deleted_total` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_fetched_total` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_inserted_total` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_returned_total` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_updated_total` tagged with db, host, server, user (counter)
- `pg_stat_database_xact_commit_total` tagged with db, host, server, user (counter)
- `pg_stat_database_xact_rollback_total` tagged with db, host, server, user (counter)
- `pg_database_conflicts_total` tagged by db name and server (counter)
- `pg_database_conflicts_confl_bufferpin_total` tagged by db name and server (counter)
- `pg_database_conflicts_confl_deadlock_total` tagged by db name and server (counter)
- `pg_database_conflicts_confl_lock_total tagged` by db name and server (counter)
- `pg_database_conflicts_confl_snapshot_total` tagged by db name and server (counter)
- `pg_database_conflicts_confl_tablespace_total` tagged by db name and server (counter)

Naming of metrics is determined via:

- `table_name _ column_name + Prometheus endings (_total for counters, etc)`

For example:

- `pg_database_conflicts_confl_tablespace_total`

Here `pg_database_conflicts` is the name of the table, `confl_tablespace` is the column name, and `_total` is suffixed because counters end in `total` in the [Prometheus naming convention](https://prometheus.io/docs/practices/naming/).

This is in line with the Prometheus naming convention for their exporter.

All metrics will also be tagged with the `endpoint` (stripped of username/password).

## Doc-level Proposal

The following additional source configuration will be added:

```toml
[sources.my_source_id]
  type = "postgresql_metrics" # required
  endpoint = "postgres://postgres@localhost" # required - address of the PG server.
  included_databases = ["production", "testing"] # optional, list of databases to query. Defaults to all if not specified.
  excluded_databases = [ "development" ] # optional, excludes specific databases. If a DB is excluded explicitly but included in `included_databases` then it is excluded.
  scrape_interval_secs = 15 # optional, default, seconds
  namespace = "postgresql" # optional, default is "postgresql", namespace to attach to metrics.
```

We will also expose the HTTP SSL settings and support `ssl` in the `endpoint` URL.

- We'd also add a guide for doing this without root permissions.

## Rationale

PostgreSQL is a commonly adopted modern database. Users frequently want to monitor its state and performance.

Additionally, as part of Vector's vision to be the "one tool" for ingesting and shipping observability data, it makes sense to add as many sources as possible to reduce the likelihood that a user will not be able to ingest metrics from their tools.

## Prior Art

- https://github.com/wrouesnel/postgres_exporter/
- https://github.com/influxdata/telegraf/tree/master/plugins/inputs/postgresql
- https://collectd.org/wiki/index.php/Plugin:PostgreSQL
- https://collectd.org/documentation/manpages/collectd.conf.5.shtml#plugin_postgresql

## Drawbacks

- Additional maintenance and integration testing burden of a new source

## Alternatives

### Having users run telegraf or Prom node exporter and using Vector's prometheus source to scrape it

We could not add the source directly to Vector and instead instruct users to run Telegraf's `postgresql` input or Prometheus' `postgresql_exporter` and point Vector at the resulting data. This would leverage the already supported inputs from those projects.

I decided against this as it would be in contrast with one of the listed
principles of Vector:

> One Tool. All Data. - One simple tool gets your logs, metrics, and traces
> (coming soon) from A to B.

[Vector
principles](https://vector.dev/docs/about/what-is-vector/#who-should-use-vector)

On the same page, it is mentioned that Vector should be a replacement for
Telegraf.

> You SHOULD use Vector to replace Logstash, Fluent*, Telegraf, Beats, or
> similar tools.

If users are already running Telegraf or PostgreSQL Exporter though, they could opt for this path.

## Outstanding Questions

- Grab pg_settings - should look at this during implementation.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with the initial source implementation

## Future Work

- Extend source to collect additional database metrics:
  - Replication
  - Locks
  - pg_stat_user_tables
