use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    internal_events::{PostgresqlMetricsCollectCompleted, PostgresqlMetricsCollectFailed},
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use chrono::{DateTime, Utc};
use futures::{
    future::{join_all, try_join_all},
    stream, FutureExt, SinkExt, StreamExt, TryFutureExt,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::BTreeMap, future::ready, time::Instant};
use tokio::time;
use tokio_postgres::{
    config::{ChannelBinding, Host, SslMode, TargetSessionAttrs},
    types::FromSql,
    Client, Config, Error as PgError, NoTls, Row,
};

macro_rules! tags {
    ($tags:expr) => { $tags.clone() };
    ($tags:expr, $($key:expr => $value:expr),*) => {
        {
            let mut tags = $tags.clone();
            $(
                tags.insert($key.into(), $value.into());
            )*
            tags
        }
    };
}

macro_rules! counter {
    ($value:expr) => {
        MetricValue::Counter {
            value: $value as f64,
        }
    };
}

macro_rules! gauge {
    ($value:expr) => {
        MetricValue::Gauge {
            value: $value as f64,
        }
    };
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid endpoint: {:?}", source))]
    InvalidEndpoint { source: PgError },
    #[snafu(display("multiple hosts not supported: {:?}", hosts))]
    MultipleHostsNotSupported { hosts: Vec<Host> },
    #[snafu(display("failed to connect ({}): {:?}", endpoint, source))]
    ConnectionFailed { source: PgError, endpoint: String },
    #[snafu(display("failed to get PostgreSQL version ({}): {:?}", endpoint, source))]
    VersionFailed { source: PgError, endpoint: String },
}

#[derive(Debug, Snafu)]
pub enum CollectError {
    #[snafu(display("failed to get value by key: {} (reason: {})", key, source))]
    PostgresGetValue { source: PgError, key: &'static str },
    #[snafu(display("query failed: {}", source))]
    QueryError { source: PgError },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
struct PostgresqlMetricsConfig {
    endpoints: Vec<String>,
    // TODO: use included/excluded
    included_databases: Vec<String>,
    excluded_databases: Vec<String>,
    scrape_interval_secs: u64,
    namespace: String,
    // TODO: SSL
}

impl Default for PostgresqlMetricsConfig {
    fn default() -> Self {
        Self {
            endpoints: vec![],
            included_databases: vec![],
            excluded_databases: vec![],
            scrape_interval_secs: 15,
            namespace: "postgresql".to_owned(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<PostgresqlMetricsConfig>("postgresql_metrics")
}

impl_generate_config_from_default!(PostgresqlMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "postgresql_metrics")]
impl SourceConfig for PostgresqlMetricsConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());

        let sources = try_join_all(
            self.endpoints
                .iter()
                .map(|endpoint| PostgresqlMetrics::new(endpoint.clone(), namespace.clone())),
        )
        .await?;

        let mut out =
            out.sink_map_err(|error| error!(message = "Error sending mongodb metrics.", %error));

        let duration = time::Duration::from_secs(self.scrape_interval_secs);
        Ok(Box::pin(async move {
            let mut interval = time::interval(duration).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter().map(|source| source.collect())).await;
                emit!(PostgresqlMetricsCollectCompleted {
                    start,
                    end: Instant::now()
                });

                let mut stream = stream::iter(metrics).flatten().map(Event::Metric).map(Ok);
                out.send_all(&mut stream).await?;
            }

            Ok(())
        }))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "postgresql_metrics"
    }
}

#[derive(Debug)]
struct PostgresqlMetrics {
    endpoint: String,
    client: Client,
    namespace: Option<String>,
    tags: BTreeMap<String, String>,
}

impl PostgresqlMetrics {
    async fn new(endpoint: String, namespace: Option<String>) -> Result<Self, BuildError> {
        let config: Config = endpoint.parse().context(InvalidEndpoint)?;
        if config.get_hosts().len() > 1 {
            return Err(BuildError::MultipleHostsNotSupported {
                hosts: config.get_hosts().to_owned(),
            });
        }

        // TODO: Tls support
        let (client, connection) =
            config
                .connect(NoTls)
                .await
                .with_context(|| ConnectionFailed {
                    endpoint: config_to_endpoint(&config),
                })?;

        // TODO:
        // 1) need shutdown?
        // 2) remove unwrap()
        // 3) reconnect
        tokio::spawn(async move {
            // if let Err(e) = connection.await {
            //     eprintln!("connection error: {}", e);
            // }
            let result = connection.await;
            println!("connection finished");
            result.unwrap();
        });

        let version_row = client
            .query_one("SELECT version()", &[])
            .await
            .with_context(|| VersionFailed {
                endpoint: config_to_endpoint(&config),
            })?;
        let version = version_row
            .try_get::<&str, &str>("version")
            .with_context(|| VersionFailed {
                endpoint: config_to_endpoint(&config),
            })?;
        debug!(message = "Connected to server.", endpoint = %config_to_endpoint(&config), server_version = %version);

        let mut tags = BTreeMap::new();
        tags.insert("endpoint".into(), config_to_endpoint(&config));
        tags.insert(
            "host".into(),
            match &config.get_hosts()[0] {
                Host::Tcp(host) => host.clone(),
                #[cfg(unix)]
                Host::Unix(path) => path.to_string_lossy().to_string(),
            },
        );

        Ok(Self {
            endpoint,
            client,
            namespace,
            tags,
        })
    }

    async fn collect(&self) -> stream::BoxStream<'static, Metric> {
        let (up_value, metrics) = match self.collect_metrics().await {
            Ok(metrics) => (1.0, stream::iter(metrics).boxed()),
            Err(error) => {
                emit!(PostgresqlMetricsCollectFailed {
                    error,
                    endpoint: self.tags.get("endpoint").expect("should be defined"),
                });
                (0.0, stream::empty().boxed())
            }
        };

        let up_metric = self.create_metric("up", gauge!(up_value), tags!(self.tags));
        stream::once(ready(up_metric)).chain(metrics).boxed()
    }

    async fn collect_metrics(&self) -> Result<impl Iterator<Item = Metric>, CollectError> {
        try_join_all(vec![
            self.collect_pg_stat_database().boxed(),
            self.collect_pg_stat_database_conflicts().boxed(),
            self.collect_pg_stat_bgwriter().boxed(),
        ])
        .map_ok(|metrics| metrics.into_iter().flatten())
        .await
    }

    async fn collect_pg_stat_database(&self) -> Result<Vec<Metric>, CollectError> {
        let rows = self
            .client
            .query("SELECT * FROM pg_stat_database", &[])
            .await
            .context(QueryError)?;

        let mut metrics = Vec::with_capacity(20 * rows.len());
        for row in rows.iter() {
            let db = row_get_value::<Option<&str>>(row, "datname")?.unwrap_or("");

            metrics.extend_from_slice(&[
                self.create_metric(
                    "pg_stat_database_datid",
                    gauge!(row_get_value::<u32>(row, "datid")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_numbackends",
                    gauge!(row_get_value::<i32>(row, "numbackends")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_xact_commit_total",
                    counter!(row_get_value::<i64>(row, "xact_commit")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_xact_rollback_total",
                    counter!(row_get_value::<i64>(row, "xact_rollback")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_blks_read_total",
                    counter!(row_get_value::<i64>(row, "blks_read")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_blks_hit_total",
                    counter!(row_get_value::<i64>(row, "blks_hit")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_tup_returned_total",
                    counter!(row_get_value::<i64>(row, "tup_returned")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_tup_fetched_total",
                    counter!(row_get_value::<i64>(row, "tup_fetched")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_tup_inserted_total",
                    counter!(row_get_value::<i64>(row, "tup_inserted")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_tup_updated_total",
                    counter!(row_get_value::<i64>(row, "tup_updated")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_tup_deleted_total",
                    counter!(row_get_value::<i64>(row, "tup_deleted")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_conflicts_total",
                    counter!(row_get_value::<i64>(row, "conflicts")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_temp_files_total",
                    counter!(row_get_value::<i64>(row, "temp_files")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_temp_bytes_total",
                    counter!(row_get_value::<i64>(row, "temp_bytes")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_deadlocks_total",
                    counter!(row_get_value::<i64>(row, "deadlocks")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_checksum_failures_total",
                    counter!(row_get_value::<Option<i64>>(row, "checksum_failures")?.unwrap_or(0)),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_checksum_last_failure",
                    gauge!(
                        row_get_value::<Option<DateTime<Utc>>>(row, "checksum_last_failure")?
                            .map(|t| t.timestamp())
                            .unwrap_or(0)
                    ),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_blk_read_time_seconds_total",
                    counter!(row_get_value::<f64>(row, "blk_read_time")? / 1000f64),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_blk_write_time_seconds_total",
                    counter!(row_get_value::<f64>(row, "blk_write_time")? / 1000f64),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_stats_reset",
                    gauge!(row_get_value::<Option<DateTime<Utc>>>(row, "stats_reset")?
                        .map(|t| t.timestamp())
                        .unwrap_or(0)),
                    tags!(self.tags, "db" => db),
                ),
            ]);
        }
        Ok(metrics)
    }

    async fn collect_pg_stat_database_conflicts(&self) -> Result<Vec<Metric>, CollectError> {
        let rows = self
            .client
            .query("SELECT * FROM pg_stat_database_conflicts", &[])
            .await
            .context(QueryError)?;

        let mut metrics = Vec::with_capacity(5 * rows.len());
        for row in rows.iter() {
            let db = row_get_value::<&str>(row, "datname")?;

            metrics.extend_from_slice(&[
                self.create_metric(
                    "pg_stat_database_conflicts_confl_tablespace_total",
                    counter!(row_get_value::<i64>(row, "confl_tablespace")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_conflicts_confl_lock_total",
                    counter!(row_get_value::<i64>(row, "confl_lock")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_conflicts_confl_snapshot_total",
                    counter!(row_get_value::<i64>(row, "confl_snapshot")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_conflicts_confl_bufferpin_total",
                    counter!(row_get_value::<i64>(row, "confl_bufferpin")?),
                    tags!(self.tags, "db" => db),
                ),
                self.create_metric(
                    "pg_stat_database_conflicts_confl_deadlock_total",
                    counter!(row_get_value::<i64>(row, "confl_deadlock")?),
                    tags!(self.tags, "db" => db),
                ),
            ]);
        }
        Ok(metrics)
    }

    async fn collect_pg_stat_bgwriter(&self) -> Result<Vec<Metric>, CollectError> {
        let row = self
            .client
            .query_one("SELECT * FROM pg_stat_bgwriter", &[])
            .await
            .context(QueryError)?;

        Ok(vec![
            self.create_metric(
                "pg_stat_bgwriter_checkpoints_timed_total",
                counter!(row_get_value::<i64>(&row, "checkpoints_timed")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_checkpoints_req_total",
                counter!(row_get_value::<i64>(&row, "checkpoints_req")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_checkpoint_write_time_seconds_total",
                counter!(row_get_value::<f64>(&row, "checkpoint_write_time")? / 1000f64),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_checkpoint_sync_time_seconds_total",
                counter!(row_get_value::<f64>(&row, "checkpoint_sync_time")? / 1000f64),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_buffers_checkpoint_total",
                counter!(row_get_value::<i64>(&row, "buffers_checkpoint")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_buffers_clean_total",
                counter!(row_get_value::<i64>(&row, "buffers_clean")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_maxwritten_clean_total",
                counter!(row_get_value::<i64>(&row, "maxwritten_clean")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_buffers_backend_total",
                counter!(row_get_value::<i64>(&row, "buffers_backend")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_buffers_backend_fsync_total",
                counter!(row_get_value::<i64>(&row, "buffers_backend_fsync")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_buffers_alloc_total",
                counter!(row_get_value::<i64>(&row, "buffers_alloc")?),
                tags!(self.tags),
            ),
            self.create_metric(
                "pg_stat_bgwriter_stats_reset",
                gauge!(row_get_value::<DateTime<Utc>>(&row, "stats_reset")?.timestamp()),
                tags!(self.tags),
            ),
        ])
    }

    fn create_metric(
        &self,
        name: &str,
        value: MetricValue,
        tags: BTreeMap<String, String>,
    ) -> Metric {
        Metric {
            name: name.into(),
            namespace: self.namespace.clone(),
            timestamp: Some(Utc::now()),
            tags: Some(tags),
            kind: MetricKind::Absolute,
            value,
        }
    }
}

fn row_get_value<'a, T: FromSql<'a>>(row: &'a Row, key: &'static str) -> Result<T, CollectError> {
    row.try_get::<&str, T>(key)
        .map_err(|source| CollectError::PostgresGetValue { source, key })
}

fn config_to_endpoint(config: &Config) -> String {
    let mut params: Vec<(&'static str, String)> = vec![];

    // options
    if let Some(options) = config.get_options() {
        params.push(("options", options.to_string()));
    }

    // application_name
    if let Some(name) = config.get_application_name() {
        params.push(("application_name", name.to_string()));
    }

    // ssl_mode, ignore default value (SslMode::Prefer)
    match config.get_ssl_mode() {
        SslMode::Disable => params.push(("ssl_mode", "disable".to_string())),
        SslMode::Prefer => {} // default, ignore
        SslMode::Require => params.push(("ssl_mode", "require".to_string())),
        _ => {} // non_exhaustive
    };

    // host
    for host in config.get_hosts() {
        match host {
            Host::Tcp(host) => params.push(("host", host.to_string())),
            #[cfg(unix)]
            Host::Unix(path) => params.push(("host", path.to_string_lossy().to_string())),
        }
    }

    // port
    for port in config.get_ports() {
        params.push(("port", port.to_string()));
    }

    // connect_timeout
    if let Some(connect_timeout) = config.get_connect_timeout() {
        params.push(("connect_timeout", connect_timeout.as_secs().to_string()));
    }

    // keepalives, ignore default value (true)
    if !config.get_keepalives() {
        params.push(("keepalives", "1".to_owned()));
    }

    // keepalives_idle, ignore default value (2 * 60 * 60)
    let keepalives_idle = config.get_keepalives_idle().as_secs();
    if keepalives_idle != 2 * 60 * 60 {
        params.push(("keepalives_idle", keepalives_idle.to_string()));
    }

    // target_session_attrs, ignore default value (TargetSessionAttrs::Any)
    match config.get_target_session_attrs() {
        TargetSessionAttrs::Any => {} // default, ignore
        TargetSessionAttrs::ReadWrite => {
            params.push(("target_session_attrs", "read-write".to_owned()))
        }
        _ => {} // non_exhaustive
    }

    // channel_binding, ignore default value (ChannelBinding::Prefer)
    match config.get_channel_binding() {
        ChannelBinding::Disable => params.push(("channel_binding", "disable".to_owned())),
        ChannelBinding::Prefer => {} // default, ignore
        ChannelBinding::Require => params.push(("channel_binding", "require".to_owned())),
        _ => {} // non_exhaustive
    }

    format!(
        "postgresql:///{}?{}",
        config.get_dbname().unwrap_or(""),
        params
            .into_iter()
            .map(|(k, v)| format!(
                "{}={}",
                percent_encoding(k.as_bytes()),
                percent_encoding(v.as_bytes())
            ))
            .collect::<Vec<String>>()
            .join("&")
    )
}

fn percent_encoding(input: &'_ [u8]) -> String {
    percent_encoding::percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PostgresqlMetricsConfig>();
    }
}

#[cfg(all(test, feature = "postgresql_metrics-integration-tests"))]
mod integration_tests {
    // use super::*;

    //
}
