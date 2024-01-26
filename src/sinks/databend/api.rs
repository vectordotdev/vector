use std::collections::BTreeMap;

use bytes::Bytes;
use http::Request;
use http::StatusCode;
use hyper::Body;
use serde::{Deserialize, Serialize};

use crate::{http::Auth, http::HttpClient, sinks::util::UriSerde};

use super::error::DatabendError;

#[derive(Serialize, Debug)]
pub(super) struct StageAttachment {
    location: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    file_format_options: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    copy_options: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Debug)]
pub(super) struct DatabendHttpRequest {
    sql: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    stage_attachment: Option<StageAttachment>,
}

impl DatabendHttpRequest {
    pub const fn new(sql: String) -> Self {
        Self {
            sql,
            stage_attachment: None,
        }
    }

    pub fn add_stage_attachment(
        &mut self,
        location: String,
        file_format_options: Option<BTreeMap<String, String>>,
        copy_options: Option<BTreeMap<String, String>>,
    ) {
        self.stage_attachment = Some(StageAttachment {
            location,
            file_format_options,
            copy_options,
        });
    }
}

#[cfg(all(test, feature = "databend-integration-tests"))]
#[derive(Deserialize, Debug)]
pub(super) struct DatabendHttpResponseSchemaField {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub(super) struct DatabendHttpResponseError {
    pub code: u16,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub(super) struct DatabendHttpResponse {
    #[cfg(all(test, feature = "databend-integration-tests"))]
    pub schema: Vec<DatabendHttpResponseSchemaField>,
    pub data: Vec<Vec<String>>,
    pub error: Option<DatabendHttpResponseError>,
    pub next_uri: Option<String>,
}

#[derive(Debug)]
pub(super) struct DatabendPresignedResponse {
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub url: String,
}

#[derive(Clone)]
pub(super) struct DatabendAPIClient {
    client: HttpClient,
    host: UriSerde,
    auth: Option<Auth>,
}

impl DatabendAPIClient {
    pub(super) const fn new(client: HttpClient, host: UriSerde, auth: Option<Auth>) -> Self {
        Self { client, host, auth }
    }

    pub(super) fn get_protocol(&self) -> &str {
        self.host.uri.scheme_str().unwrap_or("http")
    }

    pub(super) fn get_host(&self) -> &str {
        self.host.uri.host().unwrap_or("unknown")
    }

    fn get_page_endpoint(&self, next_uri: &str) -> Result<String, DatabendError> {
        let api_uri = self.host.append_path(next_uri)?;
        Ok(api_uri.to_string())
    }

    fn get_query_endpoint(&self) -> Result<String, DatabendError> {
        let api_uri = self.host.append_path("/v1/query")?;
        Ok(api_uri.to_string())
    }

    async fn do_request(
        &self,
        mut request: Request<Body>,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        if let Some(a) = &self.auth {
            a.apply(&mut request);
        }
        let response = self.client.send(request).await?;
        let status_code = response.status();
        let body_bytes = hyper::body::to_bytes(response.into_body()).await?;
        if status_code != StatusCode::OK {
            return Err(DatabendError::Server {
                code: status_code.as_u16(),
                message: format!(
                    "Unexpected status code {} with response: {}",
                    status_code,
                    String::from_utf8_lossy(&body_bytes),
                ),
            });
        }
        let resp: DatabendHttpResponse =
            serde_json::from_slice(&body_bytes).map_err(|e| DatabendError::Server {
                code: 0,
                message: format!(
                    "Failed to parse response: {}: {}",
                    e,
                    String::from_utf8_lossy(&body_bytes),
                ),
            })?;
        match resp.error {
            Some(err) => Err(DatabendError::Server {
                code: err.code,
                message: err.message,
            }),
            None => Ok(resp),
        }
    }

    pub(super) async fn query_page(
        &self,
        next_uri: String,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        let endpoint = self.get_page_endpoint(&next_uri)?;
        let request = Request::get(endpoint)
            .header("Content-Type", "application/json")
            .body(Body::empty())?;
        self.do_request(request).await
    }

    pub(super) async fn query(
        &self,
        req: DatabendHttpRequest,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        let endpoint = self.get_query_endpoint()?;
        let request = Request::post(endpoint)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&req)?))?;
        let resp = self.do_request(request).await?;
        match resp.next_uri {
            None => Ok(resp),
            Some(_) => {
                let mut resp = resp;
                let mut next_uri = resp.next_uri.clone();
                while let Some(uri) = next_uri {
                    let next_resp = self.query_page(uri).await?;
                    resp.data.extend(next_resp.data);
                    next_uri = next_resp.next_uri.clone();
                }
                Ok(resp)
            }
        }
    }

    pub(super) async fn upload_with_presigned(
        &self,
        presigned: DatabendPresignedResponse,
        data: Bytes,
    ) -> Result<(), DatabendError> {
        let req_body = Body::from(data);
        let upload_method = presigned.method.as_str();
        let mut req_builder = match upload_method {
            "PUT" => Ok(Request::put(presigned.url)),
            "POST" => Ok(Request::post(presigned.url)),
            _ => Err(DatabendError::Server {
                code: 405,
                message: format!("Unsupported presigned upload method: {}", upload_method),
            }),
        }?;
        for (k, v) in presigned.headers {
            req_builder = req_builder.header(k, v);
        }
        let request = req_builder.body(req_body)?;
        let response = self.client.send(request).await?;
        let status = response.status();
        let body = hyper::body::to_bytes(response.into_body()).await?;
        match status {
            StatusCode::OK => Ok(()),
            _ => Err(DatabendError::Server {
                code: status.as_u16(),
                message: format!(
                    "Presigned Upload Failed: {}",
                    String::from_utf8_lossy(&body)
                ),
            }),
        }
    }
}
