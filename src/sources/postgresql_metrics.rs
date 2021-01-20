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
use openssl::{
    error::ErrorStack,
    ssl::{SslConnector, SslMethod},
};
use postgres_openssl::MakeTlsConnector;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{BTreeMap, HashSet},
    future::ready,
    path::PathBuf,
    time::Instant,
};
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
    #[snafu(display("invalid endpoint: {}", source))]
    InvalidEndpoint { source: PgError },
    #[snafu(display("host missing"))]
    HostMissing,
    #[snafu(display("multiple hosts not supported: {:?}", hosts))]
    MultipleHostsNotSupported { hosts: Vec<Host> },
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
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
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

        let mut out =
            out.sink_map_err(|error| error!(message = "Error sending postgresql metrics.", %error));

        let duration = time::Duration::from_secs(self.scrape_interval_secs);
        Ok(Box::pin(async move {
            let mut interval = time::interval(duration).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter_mut().map(|source| source.collect())).await;
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

#[derive(Debug, Clone)]
enum DatnameFilter {
    Include {
        need_null: bool,
        databases: Vec<String>,
    },
    Exclude {
        need_null: bool,
        databases: Vec<String>,
    },
    None,
}

impl DatnameFilter {
    fn new(include: Vec<String>, exclude: Vec<String>) -> Self {
        let exclude = exclude.into_iter().collect::<HashSet<_>>();
        let include = include
            .into_iter()
            .filter(|name| !exclude.contains(name))
            .collect::<HashSet<_>>();

        if !include.is_empty() {
            let (need_null, databases) = Self::remove_empty(include);
            return Self::Include {
                need_null,
                databases,
            };
        }

        if !exclude.is_empty() {
            let (need_null, databases) = Self::remove_empty(exclude);
            return Self::Exclude {
                need_null,
                databases,
            };
        }

        Self::None
    }

    fn remove_empty(mut names: HashSet<String>) -> (bool, Vec<String>) {
        let null = names.remove(&"".to_owned());
        (null, names.into_iter().collect())
    }

    async fn pg_stat_database(&self, client: &Client) -> Result<Vec<Row>, PgError> {
        let mut conditions = "SELECT * FROM pg_stat_database".to_owned();
        match self {
            Self::Include {
                need_null,
                databases,
            } => {
                conditions += " WHERE datname = ANY($1)";
                if *need_null {
                    conditions += " OR datname IS NULL";
                }
                client
                    .query(conditions.as_str(), &[&databases.as_slice()])
                    .await
            }
            Self::Exclude {
                need_null,
                databases,
            } => {
                conditions += " WHERE datname != ANY($1)";
                if *need_null {
                    conditions += " AND datname IS NOT NULL";
                } else {
                    conditions += " OR datname IS NULL";
                }
                client
                    .query(conditions.as_str(), &[&databases.as_slice()])
                    .await
            }
            Self::None => client.query(conditions.as_str(), &[]).await,
        }
    }

    async fn pg_stat_database_conflicts(&self, client: &Client) -> Result<Vec<Row>, PgError> {
        let mut conditions = "SELECT * FROM pg_stat_database_conflicts".to_owned();
        match self {
            Self::Include { databases, .. } => {
                conditions += " WHERE datname = ANY($1)";
                client
                    .query(conditions.as_str(), &[&databases.as_slice()])
                    .await
            }
            Self::Exclude { databases, .. } => {
                conditions += " WHERE datname != ANY($1)";
                client
                    .query(conditions.as_str(), &[&databases.as_slice()])
                    .await
            }
            Self::None => client.query(conditions.as_str(), &[]).await,
        }
    }
}

#[derive(Debug)]
struct PostgresqlMetrics {
    config: Config,
    tls_config: Option<PostgresqlMetricsTlsConfig>,
    client: Option<Client>,
    version: Option<usize>,
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
        let config: Config = endpoint.parse().context(InvalidEndpoint)?;

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

        let mut this = Self {
            config,
            tls_config,
            client: None,
            version: None,
            namespace,
            tags,
            datname_filter,
        };
        this.build_client().await?;
        Ok(this)
    }

    async fn build_client(&mut self) -> Result<(), BuildError> {
        let client = match &self.tls_config {
            Some(tls_config) => {
                let mut builder =
                    SslConnector::builder(SslMethod::tls_client()).context(TlsFailed)?;
                builder
                    .set_ca_file(tls_config.ca_file.clone())
                    .context(TlsFailed)?;
                let connector = MakeTlsConnector::new(builder.build());

                let (client, connection) =
                    self.config
                        .connect(connector)
                        .await
                        .with_context(|| ConnectionFailed {
                            endpoint: config_to_endpoint(&self.config),
                        })?;
                tokio::spawn(connection);
                client
            }
            None => {
                let (client, connection) =
                    self.config
                        .connect(NoTls)
                        .await
                        .with_context(|| ConnectionFailed {
                            endpoint: config_to_endpoint(&self.config),
                        })?;
                tokio::spawn(connection);
                client
            }
        };

        let version_row = client
            .query_one("SELECT version()", &[])
            .await
            .with_context(|| SelectVersionFailed {
                endpoint: config_to_endpoint(&self.config),
            })?;
        let version = version_row
            .try_get::<&str, &str>("version")
            .with_context(|| SelectVersionFailed {
                endpoint: config_to_endpoint(&self.config),
            })?;
        debug!(message = "Connected to server.", endpoint = %config_to_endpoint(&self.config), server_version = %version);

        self.client = Some(client);
        self.verify_version().await?;

        Ok(())
    }

    async fn verify_version(&mut self) -> Result<(), BuildError> {
        let row = self
            .client
            .as_ref()
            .unwrap()
            .query_one("SHOW server_version_num", &[])
            .await
            .with_context(|| SelectVersionFailed {
                endpoint: config_to_endpoint(&self.config),
            })?;

        let version = row
            .try_get::<&str, &str>("server_version_num")
            .with_context(|| SelectVersionFailed {
                endpoint: config_to_endpoint(&self.config),
            })?;

        self.version = Some(match version.parse::<usize>() {
            Ok(version) if version >= 90600 => version,
            Ok(_) | Err(_) => {
                return Err(BuildError::InvalidVersion {
                    version: version.to_string(),
                })
            }
        });

        Ok(())
    }

    async fn collect(&mut self) -> stream::BoxStream<'static, Metric> {
        let build_client = match self.client {
            Some(_) => Ok(()),
            None => self.build_client().await,
        };

        let metrics = match build_client {
            Ok(()) => self
                .collect_metrics(self.client.as_ref().expect("should exists at this point"))
                .await
                .map_err(|err| err.to_string()),
            Err(err) => Err(err.to_string()),
        };

        let (up_value, metrics) = match metrics {
            Ok(metrics) => (1.0, stream::iter(metrics).boxed()),
            Err(error) => {
                self.client = None;
                emit!(PostgresqlMetricsCollectFailed {
                    error,
                    endpoint: self.tags.get("endpoint"),
                });
                (0.0, stream::empty().boxed())
            }
        };

        let up_metric = self.create_metric("up", gauge!(up_value), tags!(self.tags));
        stream::once(ready(up_metric)).chain(metrics).boxed()
    }

    async fn collect_metrics(
        &self,
        client: &Client,
    ) -> Result<impl Iterator<Item = Metric>, CollectError> {
        try_join_all(vec![
            self.collect_pg_stat_database(client).boxed(),
            self.collect_pg_stat_database_conflicts(client).boxed(),
            self.collect_pg_stat_bgwriter(client).boxed(),
        ])
        .map_ok(|metrics| metrics.into_iter().flatten())
        .await
    }

    async fn collect_pg_stat_database(&self, client: &Client) -> Result<Vec<Metric>, CollectError> {
        let rows = self
            .datname_filter
            .pg_stat_database(client)
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
            ]);
            if self.version.expect("version is set above") >= 120000 {
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

    async fn collect_pg_stat_bgwriter(&self, client: &Client) -> Result<Vec<Metric>, CollectError> {
        let row = client
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
        Metric::new(
            name.into(),
            self.namespace.clone(),
            Some(Utc::now()),
            Some(tags),
            MetricKind::Absolute,
            value,
        )
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
    use crate::{test_util::trace_init, tls, Pipeline};

    async fn test_postgresql_metrics(endpoint: String, tls: Option<PostgresqlMetricsTlsConfig>) {
        trace_init();

        let config: Config = endpoint.parse().unwrap();
        let tags_endpoint = config_to_endpoint(&config);
        let tags_host = match config.get_hosts().get(0).unwrap() {
            Host::Tcp(host) => host.clone(),
            #[cfg(unix)]
            Host::Unix(path) => path.to_string_lossy().to_string(),
        };

        let (sender, mut recv) = Pipeline::new_test();

        tokio::spawn(async move {
            PostgresqlMetricsConfig {
                endpoints: vec![endpoint],
                tls,
                ..Default::default()
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
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
                .data
                .value,
            gauge!(1)
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
    }

    #[tokio::test]
    async fn test_host() {
        test_postgresql_metrics(
            "postgresql://vector:vector@localhost/postgres".to_owned(),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn test_local() {
        let current_dir = std::env::current_dir().unwrap();
        let socket = current_dir.join("tests/data/postgresql-local-socket");
        let endpoint = format!(
            "postgresql:///postgres?host={}&user=vector&password=vector",
            socket.to_str().unwrap()
        );
        test_postgresql_metrics(endpoint, None).await
    }

    #[tokio::test]
    async fn test_host_ssl() {
        test_postgresql_metrics(
            "postgresql://vector:vector@localhost/postgres?sslmode=require".to_owned(),
            Some(PostgresqlMetricsTlsConfig {
                ca_file: tls::TEST_PEM_CA_PATH.into(),
            }),
        )
        .await
    }
}
