# RFC 3603 - 2020-08-27 - Collecting metrics from PostgreSQL

This RFC is to introduce a new metrics source to consume metrics from PostgreSQL database servers. The high level plan is to implement one source that collects metrics from PostgreSQL server instances.

## Scope

This RFC will cover:

- A new source for PostgreSQL server metrics.

This RFC will not cover:

- Other databases.

## Motivation

Users want to collect, transform, and forward metrics to better observe how their PostgreSQL databases are performing.

## Internal Proposal

Build a single source called `postgresql_metrics` (name to be confirmed) to collect PostgreSQL metrics.

The recommended implementation is to use the Rust PostgreSQL client to connect the target database server by address specified in configuration.

- https://docs.rs/postgres/0.17.5/postgres/index.html

The source would then run the following queries:

- `SELECT * FROM pg_stat_database`
- `SELECT * FROM pg_stat_database_conflicts`
- `SELECT * FROM pg_stat_bgwriter`

And return these metrics:

- `postgresql_up` -> Used as an uptime metric (0/1) ? - merits a broader discussion.
- `pg_stat_database_blk_read_time` tagged with db, host, server, user (counter)
- `pg_stat_database_blk_write_time` tagged with db, host, server, user (counter)
- `pg_stat_database_blks_hit` tagged with db, host, server, user (counter)
- `pg_stat_database_blks_read` tagged with db, host, server, user (counter)
- `pg_stat_database_stats_reset` tagged with db, host, server, user(counter)
- `pg_stat_bgwriter_buffers_alloc` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_buffers_backend` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_buffers_backend_fsync` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_buffers_checkpoint` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_buffers_clean` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_checkpoint_sync_time` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_checkpoint_write_time` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_checkpoints_req` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_checkpoints_time` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_maxwritten_clean` tagged with db, host, server, user (counter)
- `pg_stat_bgwriter_stats_reset` (counter)
- `pg_stat_database_conflicts` tagged with db, host, server, user (counter)
- `pg_stat_database_datid` tagged with db, host, server, user (counter)
- `pg_stat_database_deadlocks` tagged with db, host, server, user (counter)
- `pg_stat_database_numbackends` tagged with db, host, server, user (gauge)
- `pg_stat_database_temp_bytes` tagged with db, host, server, user (counter)
- `pg_stat_database_temp_files` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_deleted` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_fetched` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_inserted` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_returned` tagged with db, host, server, user (counter)
- `pg_stat_database_tup_updated` tagged with db, host, server, user (counter)
- `pg_stat_database_xact_commit` tagged with db, host, server, user (counter)
- `pg_stat_database_xact_rollback` tagged with db, host, server, user (counter)
- `postgresql_database_conflicts` tagged by db name and server (counter)
- `postgresql_database_conflicts_confl_bufferpin` tagged by db name and server (counter)
- `postgresql_database_conflicts_confl_deadlock` tagged by db name and server (counter)
- `postgresql_database_conflicts_confl_lock tagged` by db name and server (counter)
- `postgresql_database_conflicts_confl_snapshot` tagged by db name and server (counter)
- `postgresql_database_conflicts_confl_tablespace` tagged by db name and server (counter)

Naming of metrics is determined via:

- `pg _ db_name _ column_name`

This is in line with the Prometheus naming convention for their exporter.

## Doc-level Proposal

The following additional source configuration will be added:

```toml
[sources.my_source_id]
  type = "postgresql_metrics" # required
  address = "postgres://postgres@localhost" # required - address of the PG server.
  databases = ["production", "testing"] # optional, list of databases to query. Defaults to all if not specified.
  scrape_interval_secs = 15 # optional, default, seconds
  namespace = "postgresql" # optional, default is "postgresql", namespace to put metrics under
```

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

We could not add the source directly to Vector and instead instruct users to run Telegraf's ``postgresl` input or Prometheus' `postgresql_exporter` and point Vector at the resulting data. This would leverage the already supported inputs from those projects.

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

- SSL. Configure? Default to disable?
- Supported PG versions? There are some differences in functionality between the versions.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with the initial source implementation

## Future Work

- Extend source to collect additional database metrics
