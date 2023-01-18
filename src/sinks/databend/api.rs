use std::collections::BTreeMap;

use bytes::Bytes;
use http::Request;
use http::StatusCode;
use hyper::Body;
use serde::{Deserialize, Serialize};

use crate::http::Auth;
use crate::http::HttpClient;
use crate::sinks::util::UriSerde;

use super::error::DatabendError;

#[derive(Serialize, Debug)]
pub struct StageAttachment {
    location: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    file_format_options: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    copy_options: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Debug)]
pub struct DatabendHttpRequest {
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

#[derive(Deserialize, Debug)]
pub struct DatabendHttpResponseSchemaField {
    pub name: String,
    pub r#type: String,
}

#[derive(Deserialize, Debug)]
pub struct DatabendHttpResponseError {
    pub code: i64,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct DatabendHttpResponse {
    pub id: String,
    pub session_id: Option<String>,
    pub session: Option<BTreeMap<String, String>>,
    pub schema: Vec<DatabendHttpResponseSchemaField>,
    pub data: Vec<Vec<String>>,
    pub state: String,
    pub error: Option<DatabendHttpResponseError>,
    // pub stats: BTreeMap<String, String>,
    // pub affect: Option<String>,
    pub stats_uri: Option<String>,
    pub final_uri: Option<String>,
    pub next_uri: Option<String>,
    pub kill_uri: Option<String>,
}

#[derive(Debug)]
pub struct DatabendPresignedResponse {
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub url: String,
}

#[derive(Clone)]
pub struct DatabendAPIClient {
    client: HttpClient,
    endpoint: UriSerde,
    auth: Option<Auth>,
}

impl DatabendAPIClient {
    pub const fn new(client: HttpClient, endpoint: UriSerde, auth: Option<Auth>) -> Self {
        Self {
            client,
            endpoint,
            auth,
        }
    }

    fn get_page_url(&self, next_uri: &str) -> Result<String, DatabendError> {
        let api_uri = self.endpoint.append_path(next_uri)?;
        Ok(api_uri.to_string())
    }

    fn get_query_url(&self) -> Result<String, DatabendError> {
        let api_uri = self.endpoint.append_path("/v1/query")?;
        Ok(api_uri.to_string())
    }

    async fn do_request(
        &self,
        url: String,
        req: Option<DatabendHttpRequest>,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        let body = match req {
            Some(r) => {
                let body = serde_json::to_vec(&r)?;
                Body::from(body)
            }
            None => Body::empty(),
        };
        let mut request = Request::post(url)
            .header("Content-Type", "application/json")
            .body(body)?;
        if let Some(a) = &self.auth {
            a.apply(&mut request);
        }
        let response = self.client.send(request).await?;
        if response.status() != StatusCode::OK {
            return Err(DatabendError::Server {
                code: response.status().as_u16() as i64,
                message: "Http Status not OK".to_string(),
            });
        }
        let body_bytes = hyper::body::to_bytes(response.into_body()).await?;
        let resp: DatabendHttpResponse =
            serde_json::from_slice(&body_bytes).map_err(|e| DatabendError::Server {
                code: 0,
                message: format!(
                    "Failed to parse response: {}: {}",
                    e,
                    String::from_utf8_lossy(&body_bytes)
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

    pub async fn query_page(
        &self,
        next_uri: String,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        let url = self.get_page_url(&next_uri)?;
        self.do_request(url, None).await
    }

    pub async fn query(
        &self,
        req: DatabendHttpRequest,
    ) -> Result<DatabendHttpResponse, DatabendError> {
        let url = self.get_query_url()?;
        let resp = self.do_request(url, Some(req)).await?;
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

    pub async fn upload_with_presigned(
        &self,
        presigned: DatabendPresignedResponse,
        data: Bytes,
    ) -> Result<(), DatabendError> {
        let req_body = Body::from(data);
        let request = Request::put(presigned.url).body(req_body)?;
        let response = self.client.send(request).await?;
        match response.status() {
            StatusCode::OK => Ok(()),
            _ => Err(DatabendError::Server {
                code: response.status().as_u16() as i64,
                message: "Presigned Upload Failed".to_string(),
            }),
        }
    }
}
