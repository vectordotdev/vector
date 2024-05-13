use std::sync::Arc;

use vector_lib::request_metadata::RequestMetadata;
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::prelude::*;
use greptimedb_client::api::v1::auth_header::AuthScheme;
use greptimedb_client::api::v1::*;
use greptimedb_client::channel_manager::*;
use greptimedb_client::{Client, Database, Error as GreptimeError};

use self::logs::config::GreptimeDBLogsConfig;
use self::metrics::GreptimeDBConfig;

// sub level implementations
mod logs;
mod metrics;

mod request;

fn default_dbname() -> String {
    greptimedb_client::DEFAULT_SCHEMA_NAME.to_string()
}

#[derive(Clone, Copy, Debug, Default)]
struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone, Default)]
struct GreptimeDBRetryLogic;

impl RetryLogic for GreptimeDBRetryLogic {
    type Response = GreptimeDBBatchOutput;
    type Error = GreptimeError;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}

#[derive(Debug)]
struct GreptimeDBBatchOutput {
    pub _item_count: u32,
    pub metadata: RequestMetadata,
}

impl DriverResponse for GreptimeDBBatchOutput {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

#[derive(Debug, Clone)]
struct GreptimeDBService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

fn new_client_from_config(config: &GreptimeDBServiceConfig) -> crate::Result<Client> {
    if let Some(tls_config) = &config.tls {
        let channel_config = ChannelConfig {
            client_tls: Some(try_from_tls_config(tls_config)?),
            ..Default::default()
        };
        Ok(Client::with_manager_and_urls(
            ChannelManager::with_tls_config(channel_config).map_err(Box::new)?,
            vec![&config.endpoint],
        ))
    } else {
        Ok(Client::with_urls(vec![&config.endpoint]))
    }
}

fn try_from_tls_config(tls_config: &TlsConfig) -> crate::Result<ClientTlsOption> {
    if tls_config.key_pass.is_some()
        || tls_config.alpn_protocols.is_some()
        || tls_config.verify_certificate.is_some()
        || tls_config.verify_hostname.is_some()
    {
        warn!(message = "TlsConfig: key_pass, alpn_protocols, verify_certificate and verify_hostname are not supported by greptimedb client at the moment.");
    }

    Ok(ClientTlsOption {
        server_ca_cert_path: tls_config.ca_file.clone(),
        client_cert_path: tls_config.crt_file.clone(),
        client_key_path: tls_config.key_file.clone(),
    })
}

impl GreptimeDBService {
    pub fn try_new(config: impl Into<GreptimeDBServiceConfig>) -> crate::Result<Self> {
        let config = config.into();

        let grpc_client = new_client_from_config(&config)?;

        let mut client = Database::new_with_dbname(&config.dbname, grpc_client);

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client.set_auth(AuthScheme::Basic(Basic {
                username: username.to_owned(),
                password: password.clone().into(),
            }))
        };

        Ok(GreptimeDBService {
            client: Arc::new(client),
        })
    }
}

struct GreptimeDBServiceConfig {
    endpoint: String,
    dbname: String,
    username: Option<String>,
    password: Option<SensitiveString>,
    tls: Option<TlsConfig>,
}

impl From<&GreptimeDBLogsConfig> for GreptimeDBServiceConfig {
    fn from(val: &GreptimeDBLogsConfig) -> Self {
        GreptimeDBServiceConfig {
            endpoint: val.endpoint.clone(),
            dbname: val.dbname.clone(),
            username: val.username.clone(),
            password: val.password.clone(),

            tls: val.tls.clone(),
        }
    }
}

impl From<&GreptimeDBConfig> for GreptimeDBServiceConfig {
    fn from(val: &GreptimeDBConfig) -> Self {
        GreptimeDBServiceConfig {
            endpoint: val.endpoint.clone(),
            dbname: val.dbname.clone(),
            username: val.username.clone(),
            password: val.password.clone(),
            tls: val.tls.clone(),
        }
    }
}

fn healthcheck(config: impl Into<GreptimeDBServiceConfig>) -> crate::Result<Healthcheck> {
    let config = config.into();
    let client = new_client_from_config(&config)?;

    Ok(async move { client.health_check().await.map_err(|error| error.into()) }.boxed())
}

fn f64_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Field as i32,
        datatype: ColumnDataType::Float64 as i32,
        ..Default::default()
    }
}

fn ts_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Timestamp as i32,
        datatype: ColumnDataType::TimestampMillisecond as i32,
        ..Default::default()
    }
}

fn tag_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Tag as i32,
        datatype: ColumnDataType::String as i32,
        ..Default::default()
    }
}

fn str_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Field as i32,
        datatype: ColumnDataType::String as i32,
        ..Default::default()
    }
}
