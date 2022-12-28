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
    pub session_id: String,
    pub session: BTreeMap<String, String>,
    pub schema: Vec<DatabendHttpResponseSchemaField>,
    pub data: Vec<Vec<String>>,
    pub state: String,
    pub error: Option<DatabendHttpResponseError>,
    // pub stats: BTreeMap<String, String>,
    // pub affect: Option<String>,
    pub stats_uri: String,
    pub final_uri: String,
    pub next_uri: String,
    pub kill_uri: String,
}

// pub async fn query_page(
//     client: HttpClient,
//     next_url: String,
//     auth: Option<Auth>,
//     query_id: String,
// ) -> Result<DatabendHttpResponse, DatabendError> {
// }

pub async fn http_query(
    client: HttpClient,
    endpoint: UriSerde,
    auth: Option<Auth>,
    request: DatabendHttpRequest,
) -> Result<DatabendHttpResponse, DatabendError> {
    let api_uri = format!("{}v1/query", endpoint);
    let req_body = Body::from(
        crate::serde::json::to_bytes(&request)
            .map_err(|err| DatabendError::Encode {
                error: err,
                message: "query request".to_string(),
            })?
            .freeze(),
    );
    let mut request = Request::post(api_uri)
        .header("Content-Type", "application/json")
        .body(req_body)
        .map_err(|err| DatabendError::Http {
            error: err,
            message: "query request".to_string(),
        })?;
    if let Some(a) = auth {
        a.apply(&mut request);
    }
    let response = client
        .send(request)
        .await
        .map_err(|err| DatabendError::Request {
            error: err,
            message: "query request".to_string(),
        })?;

    if response.status() != StatusCode::OK {
        return Err(DatabendError::Server {
            code: response.status().as_u16() as i64,
            message: "Http Status not OK for query request".to_string(),
        });
    }

    let body_bytes = hyper::body::to_bytes(response.into_body())
        .await
        .map_err(|err| DatabendError::Hyper {
            error: err,
            message: "query response".to_string(),
        })?;

    let resp: DatabendHttpResponse =
        serde_json::from_slice(&body_bytes).map_err(|err| DatabendError::Decode {
            error: err,
            message: "query response".to_string(),
        })?;

    match resp.error {
        Some(err) => Err(DatabendError::Server {
            code: err.code,
            message: err.message,
        }),
        None => Ok(resp),
    }
}

#[derive(Debug)]
pub struct DatabendPresignedResponse {
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub url: String,
}

pub async fn upload_with_presigned(
    client: HttpClient,
    presigned: DatabendPresignedResponse,
    data: Bytes,
) -> Result<(), DatabendError> {
    let req_body = Body::from(data);
    let request =
        Request::put(presigned.url)
            .body(req_body)
            .map_err(|err| DatabendError::Http {
                error: err,
                message: "presigned upload".to_string(),
            })?;

    let response = client
        .send(request)
        .await
        .map_err(|err| DatabendError::Request {
            error: err,
            message: "presigned upload".to_string(),
        })?;

    if response.status() != StatusCode::OK {
        return Err(DatabendError::Server {
            code: response.status().as_u16() as i64,
            message: "Presigned Upload Failed".to_string(),
        });
    }

    Ok(())
}
