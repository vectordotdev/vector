use std::num::NonZeroUsize;
use std::sync::Arc;
use std::task::Poll;

use greptimedb_client::api::v1::auth_header::AuthScheme;
use greptimedb_client::api::v1::*;
use greptimedb_client::channel_manager::*;
use greptimedb_client::{Client, Database, Error as GreptimeError};
use vector_lib::event::Metric;

use crate::sinks::prelude::*;

use super::batch::GreptimeDBBatchSizer;
use super::request_builder::metric_to_insert_request;
use super::{GreptimeDBConfig, GreptimeDBConfigError};

#[derive(Clone, Default)]
pub(super) struct GreptimeDBRetryLogic;

impl RetryLogic for GreptimeDBRetryLogic {
    type Response = GreptimeDBBatchOutput;
    type Error = GreptimeError;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}

#[derive(Clone)]
pub(super) struct GreptimeDBRequest {
    items: RowInsertRequests,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl GreptimeDBRequest {
    pub(super) fn from_metrics(metrics: Vec<Metric>) -> Self {
        let mut items = Vec::with_capacity(metrics.len());
        let mut finalizers = EventFinalizers::default();
        let mut request_metadata_builder = RequestMetadataBuilder::default();

        let sizer = GreptimeDBBatchSizer;
        let mut estimated_request_size = 0;
        for mut metric in metrics.into_iter() {
            finalizers.merge(metric.take_finalizers());
            estimated_request_size += sizer.estimated_size_of(&metric);

            request_metadata_builder.track_event(metric.clone());

            items.push(metric_to_insert_request(metric));
        }

        let request_size =
            NonZeroUsize::new(estimated_request_size).expect("request should never be zero length");

        GreptimeDBRequest {
            items: RowInsertRequests { inserts: items },
            finalizers,
            metadata: request_metadata_builder.with_request_size(request_size),
        }
    }
}

impl Finalizable for GreptimeDBRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for GreptimeDBRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug)]
pub struct GreptimeDBBatchOutput {
    pub item_count: u32,
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
pub struct GreptimeDBService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

impl GreptimeDBService {
    pub fn try_new(config: &GreptimeDBConfig) -> crate::Result<Self> {
        let grpc_client = if let Some(tls_config) = &config.tls {
            let channel_config = ChannelConfig {
                client_tls: Self::try_from_tls_config(tls_config)?,
                ..Default::default()
            };
            Client::with_manager_and_urls(
                ChannelManager::with_tls_config(channel_config).map_err(Box::new)?,
                vec![&config.endpoint],
            )
        } else {
            Client::with_urls(vec![&config.endpoint])
        };

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

    fn try_from_tls_config(tls_config: &TlsConfig) -> crate::Result<Option<ClientTlsOption>> {
        if let Some(ca_path) = tls_config.ca_file.as_ref() {
            let cert_path = tls_config
                .crt_file
                .as_ref()
                .ok_or(GreptimeDBConfigError::TlsMissingCert)?;
            let key_path = tls_config
                .key_file
                .as_ref()
                .ok_or(GreptimeDBConfigError::TlsMissingKey)?;

            if tls_config.key_pass.is_some()
                || tls_config.alpn_protocols.is_some()
                || tls_config.verify_certificate.is_some()
                || tls_config.verify_hostname.is_some()
            {
                warn!(
                    message = "TlsConfig: key_pass, alpn_protocols, verify_certificate and verify_hostname are not supported by greptimedb client at the moment."
                );
            }

            Ok(Some(ClientTlsOption {
                server_ca_cert_path: ca_path.clone(),
                client_key_path: key_path.clone(),
                client_cert_path: cert_path.clone(),
            }))
        } else {
            Ok(None)
        }
    }
}

impl Service<GreptimeDBRequest> for GreptimeDBService {
    type Response = GreptimeDBBatchOutput;
    type Error = GreptimeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Convert vector metrics into GreptimeDB format and send them in batch
    fn call(&mut self, req: GreptimeDBRequest) -> Self::Future {
        let client = Arc::clone(&self.client);

        Box::pin(async move {
            let metadata = req.metadata;
            let result = client.row_insert(req.items).await?;

            Ok(GreptimeDBBatchOutput {
                item_count: result,
                metadata,
            })
        })
    }
}
