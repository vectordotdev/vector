use crate::sinks::{
    greptimedb::metrics::config::GreptimeDBMetricsConfig,
    greptimedb::metrics::request::{GreptimeDBGrpcBatchOutput, GreptimeDBGrpcRequest},
    prelude::*,
};
use greptimedb_ingester::{
    api::v1::auth_header::AuthScheme, api::v1::*, channel_manager::*, Client, ClientBuilder,
    Compression, Database, Error as GreptimeError,
};
use std::{sync::Arc, task::Poll};
use vector_lib::sensitive_string::SensitiveString;

#[derive(Debug, Clone)]
pub struct GreptimeDBGrpcService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

fn new_client_from_config(config: &GreptimeDBGrpcServiceConfig) -> crate::Result<Client> {
    let mut builder = ClientBuilder::default().peers(vec![&config.endpoint]);

    if let Some(compression) = config.compression.as_ref() {
        let compression = match compression.as_str() {
            "gzip" => Compression::Gzip,
            "zstd" => Compression::Zstd,
            _ => {
                warn!(message = "Unknown gRPC compression type: {compression}, disabled.");
                Compression::None
            }
        };
        builder = builder.compression(compression);
    }

    if let Some(tls_config) = &config.tls {
        let channel_config = ChannelConfig {
            client_tls: Some(try_from_tls_config(tls_config)?),
            ..Default::default()
        };

        builder = builder
            .channel_manager(ChannelManager::with_tls_config(channel_config).map_err(Box::new)?);
    }

    Ok(builder.build())
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
            let result = client.row_insert(req.items).await?;

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
    compression: Option<String>,
    tls: Option<TlsConfig>,
}

impl From<&GreptimeDBMetricsConfig> for GreptimeDBGrpcServiceConfig {
    fn from(val: &GreptimeDBMetricsConfig) -> Self {
        GreptimeDBGrpcServiceConfig {
            endpoint: val.endpoint.clone(),
            dbname: val.dbname.clone(),
            username: val.username.clone(),
            password: val.password.clone(),
            compression: val.grpc_compression.clone(),
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
