use std::{
    collections::{BTreeMap, HashSet},
    future::ready,
    path::PathBuf,
    time::Instant,
};

use chrono::{DateTime, Utc};
use futures::{
    future::{join_all, try_join_all},
    stream, FutureExt, StreamExt,
};
use openssl::{
    error::ErrorStack,
    ssl::{SslConnector, SslMethod},
};
use postgres_openssl::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::time;
use tokio_postgres::{
    config::{ChannelBinding, Host, SslMode, TargetSessionAttrs},
    types::FromSql,
    Client, Config, Error as PgError, NoTls, Row,
};
use tokio_stream::wrappers::IntervalStream;

use crate::{
    config::{DataType, Output, SourceConfig, SourceContext, SourceDescription},
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    internal_events::{PostgresqlMetricsCollectCompleted, PostgresqlMetricsCollectFailed},
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
    #[snafu(display("invalid endpoint: {}", source))]
    InvalidEndpoint { source: PgError },
    #[snafu(display("host missing"))]
    HostMissing,
    #[snafu(display("multiple hosts not supported: {:?}", hosts))]
    MultipleHostsNotSupported { hosts: Vec<Host> },
}

#[derive(Debug, Snafu)]
enum ConnectError {
    #[snafu(display("failed to create tls connector: {}", source))]
    TlsFailed { source: ErrorStack },
    #[snafu(display("failed to connect ({}): {}", endpoint, source))]
    ConnectionFailed { source: PgError, endpoint: String },
    #[snafu(display("failed to get PostgreSQL version ({}): {}", endpoint, source))]
    SelectVersionFailed { source: PgError, endpoint: String },
    #[snafu(display("version ({}) is not supported", version))]
    InvalidVersion { version: String },
}

#[derive(Debug, Snafu)]
enum CollectError {
    #[snafu(display("failed to get value by key: {} (reason: {})", key, source))]
    PostgresGetValue { source: PgError, key: &'static str },
    #[snafu(display("query failed: {}", source))]
    QueryError { source: PgError },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct PostgresqlMetricsTlsConfig {
    ca_file: PathBuf,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
struct PostgresqlMetricsConfig {
    endpoints: Vec<String>,
    include_databases: Option<Vec<String>>,
    exclude_databases: Option<Vec<String>>,
    scrape_interval_secs: u64,
    namespace: String,
    tls: Option<PostgresqlMetricsTlsConfig>,
}

impl Default for PostgresqlMetricsConfig {
    fn default() -> Self {
        Self {
            endpoints: vec![],
            include_databases: None,
            exclude_databases: None,
            scrape_interval_secs: 15,
            namespace: "postgresql".to_owned(),
            tls: None,
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
    async fn build(&self, mut cx: SourceContext) -> crate::Result<super::Source> {
        let datname_filter = DatnameFilter::new(
            self.include_databases.clone().unwrap_or_default(),
            self.exclude_databases.clone().unwrap_or_default(),
        );
        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());

        let mut sources = try_join_all(self.endpoints.iter().map(|endpoint| {
            PostgresqlMetrics::new(
                endpoint.clone(),
                datname_filter.clone(),
                namespace.clone(),
                self.tls.clone(),
            )
        }))
        .await?;

        let duration = time::Duration::from_secs(self.scrape_interval_secs);
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let mut interval = IntervalStream::new(time::interval(duration)).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter_mut().map(|source| source.collect())).await;
                emit!(&PostgresqlMetricsCollectCompleted {
                    start,
                    end: Instant::now()
                });

                let mut stream = stream::iter(metrics).flatten().map(Event::Metric);
                if let Err(error) = cx.out.send_all(&mut stream).await {
                    error!(message = "Error sending postgresql metrics.", %error);
                    return Err(());
                }
            }

            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        "postgresql_metrics"
    }
}

#[derive(Debug)]
struct PostgresqlClient {
    config: Config,
    tls_config: Option<PostgresqlMetricsTlsConfig>,
    client: Option<(Client, usize)>,
}

impl PostgresqlClient {
    const fn new(config: Config, tls_config: Option<PostgresqlMetricsTlsConfig>) -> Self {
        Self {
            config,
            tls_config,
            client: None,
        }
    }

    async fn take(&mut self) -> Result<(Client, usize), ConnectError> {
        match self.client.take() {
            Some((client, version)) => Ok((client, version)),
            None => self.build_client().await,
        }
    }

    fn set(&mut self, value: (Client, usize)) {
        self.client.replace(value);
    }

    async fn build_client(&self) -> Result<(Client, usize), ConnectError> {
        // Create postgresql client
        let client = match &self.tls_config {
            Some(tls_config) => {
                let mut builder =
                    SslConnector::builder(SslMethod::tls_client()).context(TlsFailedSnafu)?;
                builder
                    .set_ca_file(tls_config.ca_file.clone())
                    .context(TlsFailedSnafu)?;
                let connector = MakeTlsConnector::new(builder.build());

                let (client, connection) =
                    self.config.connect(connector).await.with_context(|_| {
                        ConnectionFailedSnafu {
                            endpoint: config_to_endpoint(&self.config),
                        }
                    })?;
                tokio::spawn(connection);
                client
            }
            None => {
                let (client, connection) =
                    self.config
                        .connect(NoTls)
                        .await
                        .with_context(|_| ConnectionFailedSnafu {
                            endpoint: config_to_endpoint(&self.config),
                        })?;
                tokio::spawn(connection);
                client
            }
        };

        // Log version if required
        if tracing::level_enabled!(tracing::Level::DEBUG) {
            let version_row = client
                .query_one("SELECT version()", &[])
                .await
                .with_context(|_| SelectVersionFailedSnafu {
                    endpoint: config_to_endpoint(&self.config),
                })?;
            let version = version_row
                .try_get::<&str, &str>("version")
                .with_context(|_| SelectVersionFailedSnafu {
                    endpoint: config_to_endpoint(&self.config),
                })?;
            debug!(message = "Connected to server.", endpoint = %config_to_endpoint(&self.config), server_version = %version);
        }

        // Get server version and check that we support it
        let row = client
            .query_one("SHOW server_version_num", &[])
            .await
            .with_context(|_| SelectVersionFailedSnafu {
                endpoint: config_to_endpoint(&self.config),
            })?;

        let version = row
            .try_get::<&str, &str>("server_version_num")
            .with_context(|_| SelectVersionFailedSnafu {
                endpoint: config_to_endpoint(&self.config),
            })?;

        let version = match version.parse::<usize>() {
            Ok(version) if version >= 90600 => version,
            Ok(_) | Err(_) => {
                return Err(ConnectError::InvalidVersion {
                    version: version.to_string(),
                })
            }
        };

        //
        Ok((client, version))
    }
}

#[derive(Debug, Clone)]
struct DatnameFilter {
    pg_stat_database_sql: String,
    pg_stat_database_conflicts_sql: String,
    match_params: Vec<String>,
}

impl DatnameFilter {
    fn new(include: Vec<String>, exclude: Vec<String>) -> Self {
        let (include_databases, include_null) = Self::clean_databases(include);
        let (exclude_databases, exclude_null) = Self::clean_databases(exclude);
        let (match_sql, match_params) =
            Self::build_match_params(include_databases, exclude_databases);

        let mut pg_stat_database_sql = "SELECT * FROM pg_stat_database".to_owned();
        if !match_sql.is_empty() {
            pg_stat_database_sql += " WHERE";
            pg_stat_database_sql += &match_sql;
        }
        match (include_null, exclude_null) {
            // Nothing
            (false, false) => {}
            // Include tracking objects not in database
            (true, false) => {
                pg_stat_database_sql += if match_sql.is_empty() {
                    " WHERE"
                } else {
                    " OR"
                };
                pg_stat_database_sql += " datname IS NULL";
            }
            // Exclude tracking objects not in database, precedence over include
            (false, true) | (true, true) => {
                pg_stat_database_sql += if match_sql.is_empty() {
                    " WHERE"
                } else {
                    " AND"
                };
                pg_stat_database_sql += " datname IS NOT NULL";
            }
        }

        let mut pg_stat_database_conflicts_sql =
            "SELECT * FROM pg_stat_database_conflicts".to_owned();
        if !match_sql.is_empty() {
            pg_stat_database_conflicts_sql += " WHERE";
            pg_stat_database_conflicts_sql += &match_sql;
        }

        Self {
            pg_stat_database_sql,
            pg_stat_database_conflicts_sql,
            match_params,
        }
    }

    fn clean_databases(names: Vec<String>) -> (Vec<String>, bool) {
        let mut set = names.into_iter().collect::<HashSet<_>>();
        let null = set.remove(&"".to_owned());
        (set.into_iter().collect(), null)
    }

    fn build_match_params(include: Vec<String>, exclude: Vec<String>) -> (String, Vec<String>) {
        let mut query = String::new();
        let mut params = vec![];

        if !include.is_empty() {
            query.push_str(" (");
            for (i, name) in include.into_iter().enumerate() {
                params.push(name);
                if i > 0 {
                    query.push_str(" OR");
                }
                query.push_str(&format!(" datname ~ ${}", params.len()));
            }
            query.push(')');
        }

        if !exclude.is_empty() {
            if !query.is_empty() {
                query.push_str(" AND");
            }

            query.push_str(" NOT (");
            for (i, name) in exclude.into_iter().enumerate() {
                params.push(name);
                if i > 0 {
                    query.push_str(" OR");
                }
                query.push_str(&format!(" datname ~ ${}", params.len()));
            }
            query.push(')');
        }

        (query, params)
    }

    fn get_match_params(&self) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            Vec::with_capacity(self.match_params.len());
        for item in self.match_params.iter() {
            params.push(item);
        }
        params
    }

    async fn pg_stat_database(&self, client: &Client) -> Result<Vec<Row>, PgError> {
        client
            .query(
                self.pg_stat_database_sql.as_str(),
                self.get_match_params().as_slice(),
            )
            .await
    }

    async fn pg_stat_database_conflicts(&self, client: &Client) -> Result<Vec<Row>, PgError> {
        client
            .query(
                self.pg_stat_database_conflicts_sql.as_str(),
                self.get_match_params().as_slice(),
            )
            .await
    }

    async fn pg_stat_bgwriter(&self, client: &Client) -> Result<Row, PgError> {
        client
            .query_one("SELECT * FROM pg_stat_bgwriter", &[])
            .await
    }
}

#[derive(Debug)]
struct PostgresqlMetrics {
    client: PostgresqlClient,
    namespace: Option<String>,
    tags: BTreeMap<String, String>,
    datname_filter: DatnameFilter,
}

impl PostgresqlMetrics {
    async fn new(
        endpoint: String,
        datname_filter: DatnameFilter,
        namespace: Option<String>,
        tls_config: Option<PostgresqlMetricsTlsConfig>,
    ) -> Result<Self, BuildError> {
        let config: Config = endpoint.parse().context(InvalidEndpointSnafu)?;

        let hosts = config.get_hosts();
        let host = match hosts.len() {
            0 => return Err(BuildError::HostMissing),
            1 => match &hosts[0] {
                Host::Tcp(host) => host.clone(),
                #[cfg(unix)]
                Host::Unix(path) => path.to_string_lossy().to_string(),
            },
            _ => {
                return Err(BuildError::MultipleHostsNotSupported {
                    hosts: config.get_hosts().to_owned(),
                })
            }
        };

        let mut tags = BTreeMap::new();
        tags.insert("endpoint".into(), config_to_endpoint(&config));
        tags.insert("host".into(), host);

        Ok(Self {
            client: PostgresqlClient::new(config, tls_config),
            namespace,
            tags,
            datname_filter,
        })
    }

    async fn collect(&mut self) -> stream::BoxStream<'static, Metric> {
        let (up_value, metrics) = match self.collect_metrics().await {
            Ok(metrics) => (1.0, stream::iter(metrics).boxed()),
            Err(error) => {
                emit!(&PostgresqlMetricsCollectFailed {
                    error,
                    endpoint: self.tags.get("endpoint"),
                });
                (0.0, stream::empty().boxed())
            }
        };

        let up_metric = self.create_metric("up", gauge!(up_value), tags!(self.tags));
        stream::once(ready(up_metric)).chain(metrics).boxed()
    }

    async fn collect_metrics(&mut self) -> Result<impl Iterator<Item = Metric>, String> {
        let (client, client_version) = self
            .client
            .take()
            .await
            .map_err(|error| error.to_string())?;

        match try_join_all(vec![
            self.collect_pg_stat_database(&client, client_version)
                .boxed(),
            self.collect_pg_stat_database_conflicts(&client).boxed(),
            self.collect_pg_stat_bgwriter(&client).boxed(),
        ])
        .await
        {
            Ok(metrics) => {
                self.client.set((client, client_version));
                Ok(metrics.into_iter().flatten())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    async fn collect_pg_stat_database(
        &self,
        client: &Client,
        client_version: usize,
    ) -> Result<Vec<Metric>, CollectError> {
        let rows = self
            .datname_filter
            .pg_stat_database(client)
            .await
            .context(QuerySnafu)?;

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
            ]);
            if client_version >= 120000 {
                metrics.extend_from_slice(&[
                    self.create_metric(
                        "pg_stat_database_checksum_failures_total",
                        counter!(
                            row_get_value::<Option<i64>>(row, "checksum_failures")?.unwrap_or(0)
                        ),
                        tags!(self.tags, "db" => db),
                    ),
                    self.create_metric(
                        "pg_stat_database_checksum_last_failure",
                        gauge!(row_get_value::<Option<DateTime<Utc>>>(
                            row,
                            "checksum_last_failure"
                        )?
                        .map(|t| t.timestamp())
                        .unwrap_or(0)),
                        tags!(self.tags, "db" => db),
                    ),
                ]);
            }
            metrics.extend_from_slice(&[
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

    async fn collect_pg_stat_database_conflicts(
        &self,
        client: &Client,
    ) -> Result<Vec<Metric>, CollectError> {
        let rows = self
            .datname_filter
            .pg_stat_database_conflicts(client)
            .await
            .context(QuerySnafu)?;

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

    async fn collect_pg_stat_bgwriter(&self, client: &Client) -> Result<Vec<Metric>, CollectError> {
        let row = self
            .datname_filter
            .pg_stat_bgwriter(client)
            .await
            .context(QuerySnafu)?;

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
        Metric::new(name, MetricKind::Absolute, value)
            .with_namespace(self.namespace.clone())
            .with_tags(Some(tags))
            .with_timestamp(Some(Utc::now()))
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
        SslMode::Disable => params.push(("sslmode", "disable".to_string())),
        SslMode::Prefer => {} // default, ignore
        SslMode::Require => params.push(("sslmode", "require".to_string())),
        // non_exhaustive enum
        _ => {
            warn!(r#"Unknown variant of "SslMode.""#);
        }
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
        // non_exhaustive enum
        _ => {
            warn!(r#"Unknown variant of "TargetSessionAttrs.""#);
        }
    }

    // channel_binding, ignore default value (ChannelBinding::Prefer)
    match config.get_channel_binding() {
        ChannelBinding::Disable => params.push(("channel_binding", "disable".to_owned())),
        ChannelBinding::Prefer => {} // default, ignore
        ChannelBinding::Require => params.push(("channel_binding", "require".to_owned())),
        // non_exhaustive enum
        _ => {
            warn!(r#"Unknown variant of "ChannelBinding"."#);
        }
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
    use super::*;
    use crate::{test_util::trace_init, tls, SourceSender};
    use std::path::PathBuf;

    fn pg_host() -> String {
        std::env::var("PG_HOST").unwrap_or_else(|_| "localhost".into())
    }

    fn pg_socket() -> PathBuf {
        std::env::var("PG_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let current_dir = std::env::current_dir().unwrap();
                current_dir
                    .join("tests")
                    .join("data")
                    .join("postgresql-local-socket")
            })
    }

    fn pg_url() -> String {
        std::env::var("PG_URL")
            .unwrap_or_else(|_| format!("postgres://vector:vector@{}/postgres", pg_host()))
    }

    async fn test_postgresql_metrics(
        endpoint: String,
        tls: Option<PostgresqlMetricsTlsConfig>,
        include_databases: Option<Vec<String>>,
        exclude_databases: Option<Vec<String>>,
    ) -> Vec<Event> {
        trace_init();

        let config: Config = endpoint.parse().unwrap();
        let tags_endpoint = config_to_endpoint(&config);
        let tags_host = match config.get_hosts().get(0).unwrap() {
            Host::Tcp(host) => host.clone(),
            #[cfg(unix)]
            Host::Unix(path) => path.to_string_lossy().to_string(),
        };

        let (sender, mut recv) = SourceSender::new_test();

        tokio::spawn(async move {
            PostgresqlMetricsConfig {
                endpoints: vec![endpoint],
                tls,
                include_databases,
                exclude_databases,
                ..Default::default()
            }
            .build(SourceContext::new_test(sender))
            .await
            .unwrap()
            .await
            .unwrap()
        });

        let event = time::timeout(time::Duration::from_secs(3), recv.next())
            .await
            .expect("fetch metrics timeout")
            .expect("failed to get metrics from a stream");
        let mut events = vec![event];
        loop {
            match time::timeout(time::Duration::from_millis(10), recv.next()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) => break,
                Err(_) => break,
            }
        }
        assert!(events.len() > 1);

        // test up metric
        assert_eq!(
            events
                .iter()
                .map(|e| e.as_metric())
                .find(|e| e.name() == "up")
                .unwrap()
                .value(),
            &gauge!(1)
        );

        // test namespace and tags
        for event in &events {
            let metric = event.as_metric();

            assert_eq!(metric.namespace(), Some("postgresql"));
            assert_eq!(
                metric.tags().unwrap().get("endpoint").unwrap(),
                &tags_endpoint
            );
            assert_eq!(metric.tags().unwrap().get("host").unwrap(), &tags_host);
        }

        // test metrics from different queries
        let names = vec![
            "pg_stat_database_datid",
            "pg_stat_database_conflicts_confl_tablespace_total",
            "pg_stat_bgwriter_checkpoints_timed_total",
        ];
        for name in names {
            assert!(events.iter().any(|e| e.as_metric().name() == name));
        }

        events
    }

    #[tokio::test]
    async fn test_host() {
        test_postgresql_metrics(pg_url(), None, None, None).await;
    }

    #[tokio::test]
    async fn test_local() {
        let endpoint = format!(
            "postgresql:///postgres?host={}&user=vector&password=vector",
            pg_socket().to_str().unwrap()
        );
        test_postgresql_metrics(endpoint, None, None, None).await;
    }

    #[tokio::test]
    async fn test_host_ssl() {
        test_postgresql_metrics(
            format!("{}?sslmode=require", pg_url()),
            Some(PostgresqlMetricsTlsConfig {
                ca_file: tls::TEST_PEM_CA_PATH.into(),
            }),
            None,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn test_host_include_databases() {
        let events = test_postgresql_metrics(
            pg_url(),
            None,
            Some(vec!["^vec".to_owned(), "gres$".to_owned()]),
            None,
        )
        .await;

        for event in events {
            let metric = event.into_metric();

            if let Some(db) = metric.tags().unwrap().get("db") {
                assert!(db == "vector" || db == "postgres");
            }
        }
    }

    #[tokio::test]
    async fn test_host_exclude_databases() {
        let events = test_postgresql_metrics(
            pg_url(),
            None,
            None,
            Some(vec!["^vec".to_owned(), "gres$".to_owned()]),
        )
        .await;

        for event in events {
            let metric = event.into_metric();

            if let Some(db) = metric.tags().unwrap().get("db") {
                assert!(db != "vector" && db != "postgres");
            }
        }
    }

    #[tokio::test]
    async fn test_host_exclude_databases_empty() {
        test_postgresql_metrics(pg_url(), None, None, Some(vec!["".to_owned()])).await;
    }

    #[tokio::test]
    async fn test_host_include_databases_and_exclude_databases() {
        let events = test_postgresql_metrics(
            pg_url(),
            None,
            Some(vec!["template\\d+".to_owned()]),
            Some(vec!["template0".to_owned()]),
        )
        .await;

        for event in events {
            let metric = event.into_metric();

            if let Some(db) = metric.tags().unwrap().get("db") {
                assert!(db == "template1");
            }
        }
    }
}
