use std::collections::BTreeMap;
use std::task::{Context, Poll};

use bytes::Bytes;
use chrono::Utc;
use futures::future::BoxFuture;
use rand::{thread_rng, Rng};
use rand_distr::Alphanumeric;
use snafu::Snafu;
use tower::Service;
use vector_lib::finalization::{EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

use crate::{internal_events::EndpointBytesSent, sinks::util::retries::RetryLogic};

use super::{
    api::{DatabendAPIClient, DatabendHttpRequest, DatabendPresignedResponse},
    error::DatabendError,
};

#[derive(Clone)]
pub struct DatabendRetryLogic;

impl RetryLogic for DatabendRetryLogic {
    type Error = DatabendError;
    type Response = DatabendResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            DatabendError::Server { code, message: _ } => match *code {
                429 => true,
                // general server error
                500 => true,
                // storage doesn't support presign operation
                3902 => false,
                // fail to parse stage attachment
                1046 => false,
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct DatabendService {
    client: DatabendAPIClient,
    database: String,
    table: String,
    file_format_options: BTreeMap<String, String>,
    copy_options: BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct DatabendRequest {
    pub data: Bytes,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl Finalizable for DatabendRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for DatabendRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug, Snafu)]
pub struct DatabendResponse {
    metadata: RequestMetadata,
}

impl DriverResponse for DatabendResponse {
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

impl DatabendService {
    pub(super) fn new(
        client: DatabendAPIClient,
        database: String,
        table: String,
        file_format_options: BTreeMap<String, String>,
        copy_options: BTreeMap<String, String>,
    ) -> Result<Self, DatabendError> {
        if database.is_empty() {
            return Err(DatabendError::InvalidConfig {
                message: "database is required".to_string(),
            });
        }
        if table.is_empty() {
            return Err(DatabendError::InvalidConfig {
                message: "table is required".to_string(),
            });
        }
        Ok(Self {
            client,
            database,
            table,
            file_format_options,
            copy_options,
        })
    }

    pub(super) fn new_stage_location(&self) -> String {
        let now = Utc::now().timestamp();
        let suffix = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect::<String>();
        format!(
            "@~/vector/{}/{}/{}-{}",
            self.database, self.table, now, suffix,
        )
    }

    pub(super) async fn get_presigned_url(
        &self,
        stage_location: &str,
    ) -> Result<DatabendPresignedResponse, DatabendError> {
        let req = DatabendHttpRequest::new(format!("PRESIGN UPLOAD {}", stage_location));
        let resp = self.client.query(req).await?;

        if resp.data.len() != 1 {
            return Err(DatabendError::Server {
                code: 500,
                message: "Empty response from server for presigned request".to_string(),
            });
        }
        if resp.data[0].len() != 3 {
            return Err(DatabendError::Server {
                code: 500,
                message: "Invalid response from server for presigned request".to_string(),
            });
        }

        // resp.data[0]: [ "PUT", "{\"host\":\"s3.us-east-2.amazonaws.com\"}", "https://s3.us-east-2.amazonaws.com/query-storage-xxxxx/tnxxxxx/stage/user/xxxx/xxx?" ]
        let method = resp.data[0][0].clone();
        let headers: BTreeMap<String, String> =
            serde_json::from_str(resp.data[0][1].clone().as_str())?;
        let url = resp.data[0][2].clone();

        if method != "PUT" {
            return Err(DatabendError::Server {
                code: 500,
                message: "Invalid method for presigned request".to_string(),
            });
        }

        Ok(DatabendPresignedResponse {
            method,
            headers,
            url,
        })
    }

    pub(crate) async fn insert_with_stage(
        &self,
        stage_location: String,
    ) -> Result<(), DatabendError> {
        let mut req = DatabendHttpRequest::new(format!(
            "INSERT INTO `{}`.`{}` VALUES",
            self.database, self.table
        ));
        req.add_stage_attachment(
            stage_location,
            Some(self.file_format_options.clone()),
            Some(self.copy_options.clone()),
        );
        _ = self.client.query(req).await?;
        Ok(())
    }
}

impl Service<DatabendRequest> for DatabendService {
    type Response = DatabendResponse;
    type Error = DatabendError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: DatabendRequest) -> Self::Future {
        let service = self.clone();

        let future = async move {
            let metadata = request.get_metadata().clone();
            let stage_location = service.new_stage_location();
            let protocol = service.client.get_protocol();
            let endpoint = service.client.get_host();
            let byte_size = request.data.len();
            let presigned_resp = service.get_presigned_url(&stage_location).await?;
            service
                .client
                .upload_with_presigned(presigned_resp, request.data)
                .await?;
            service.insert_with_stage(stage_location).await?;
            emit!(EndpointBytesSent {
                byte_size,
                protocol,
                endpoint,
            });
            Ok(DatabendResponse { metadata })
        };
        Box::pin(future)
    }
}
