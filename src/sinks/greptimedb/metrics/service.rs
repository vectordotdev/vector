use std::{sync::Arc, task::Poll};

use greptimedb_ingester::{
    Error as GreptimeError,
    api::v1::Basic,
    api::v1::auth_header::AuthScheme,
    channel_manager::{ChannelConfig, ChannelManager, ClientTlsOption},
    client::Client,
    database::Database,
};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    greptimedb::{
        GrpcCompression,
        metrics::{
            config::GreptimeDBMetricsConfig,
            request::{GreptimeDBGrpcBatchOutput, GreptimeDBGrpcRequest},
        },
    },
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct GreptimeDBGrpcService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

fn new_client_from_config(config: &GreptimeDBGrpcServiceConfig) -> crate::Result<Client> {
    let send_compression = matches!(config.compression, GrpcCompression::Zstd);

    let mut channel_config = ChannelConfig {
        send_compression,
        ..Default::default()
    };

    let channel_manager = if let Some(tls_config) = &config.tls {
        channel_config.client_tls = Some(try_from_tls_config(tls_config)?);
        ChannelManager::with_tls_config(channel_config).map_err(Box::new)?
    } else {
        ChannelManager::with_config(channel_config)
    };
    let client = Client::with_manager_and_urls(channel_manager, vec![&config.endpoint]);

    Ok(client)
}

fn try_from_tls_config(tls_config: &TlsConfig) -> crate::Result<ClientTlsOption> {
    if tls_config.key_pass.is_some()
        || tls_config.alpn_protocols.is_some()
        || tls_config.verify_certificate.is_some()
        || tls_config.verify_hostname.is_some()
    {
        warn!(
            message = "TlsConfig: key_pass, alpn_protocols, verify_certificate and verify_hostname are not supported by greptimedb client at the moment."
        );
    }

    let require_path = |p: &Option<std::path::PathBuf>, name: &str| -> crate::Result<String> {
        p.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| format!("GreptimeDB TLS requires `{name}` to be set").into())
    };

    Ok(ClientTlsOption {
        server_ca_cert_path: require_path(&tls_config.ca_file, "ca_file")?,
        client_cert_path: require_path(&tls_config.crt_file, "crt_file")?,
        client_key_path: require_path(&tls_config.key_file, "key_file")?,
    })
}

impl GreptimeDBGrpcService {
    pub fn try_new(config: impl Into<GreptimeDBGrpcServiceConfig>) -> crate::Result<Self> {
        let config = config.into();

        let grpc_client = new_client_from_config(&config)?;

        let mut client = Database::new_with_dbname(&config.dbname, grpc_client);

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client.set_auth(AuthScheme::Basic(Basic {
                username: username.to_owned(),
                password: password.clone().into(),
            }))
        };

        Ok(GreptimeDBGrpcService {
            client: Arc::new(client),
        })
    }
}

impl Service<GreptimeDBGrpcRequest> for GreptimeDBGrpcService {
    type Response = GreptimeDBGrpcBatchOutput;
    type Error = GreptimeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Convert vector metrics into GreptimeDB format and send them in batch
    fn call(&mut self, req: GreptimeDBGrpcRequest) -> Self::Future {
        let client = Arc::clone(&self.client);

        Box::pin(async move {
            let metadata = req.metadata;
            let result = client.insert(req.items).await?;

            Ok(GreptimeDBGrpcBatchOutput {
                _item_count: result,
                metadata,
            })
        })
    }
}

/// Configuration for the GreptimeDB gRPC service
pub(super) struct GreptimeDBGrpcServiceConfig {
    endpoint: String,
    dbname: String,
    username: Option<String>,
    password: Option<SensitiveString>,
    compression: GrpcCompression,
    tls: Option<TlsConfig>,
}

impl From<&GreptimeDBMetricsConfig> for GreptimeDBGrpcServiceConfig {
    fn from(val: &GreptimeDBMetricsConfig) -> Self {
        GreptimeDBGrpcServiceConfig {
            endpoint: val.endpoint.clone(),
            dbname: val.dbname.clone(),
            username: val.username.clone(),
            password: val.password.clone(),
            compression: val.grpc_compression,
            tls: val.tls.clone(),
        }
    }
}

pub(super) fn healthcheck(
    config: impl Into<GreptimeDBGrpcServiceConfig>,
) -> crate::Result<Healthcheck> {
    let config = config.into();
    let client = new_client_from_config(&config)?;

    Ok(async move { client.health_check().await.map_err(|error| error.into()) }.boxed())
}
