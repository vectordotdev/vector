//! Service implementation for the `Doris` sink.

use std::{collections::HashMap, time::SystemTime};
use bytes::Bytes;
use http::{header::{CONTENT_LENGTH, CONTENT_TYPE}, Method, Request};
use snafu::ResultExt;
use tracing::{debug};
use uuid::Uuid;
use crate::{
    http::Auth,
    sinks::{
        util::{http::{HttpRequest, HttpServiceRequestBuilder}},
        HTTPRequestBuilderSnafu,
    },
};
use super::sink::DorisPartitionKey;


/// HTTP request builder for Doris Stream Load
#[derive(Debug, Clone)]
pub struct DorisServiceRequestBuilder {
    pub auth: Option<Auth>,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub label_prefix: String,
    pub log_request: bool,
}

impl HttpServiceRequestBuilder<DorisPartitionKey> for DorisServiceRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<DorisPartitionKey>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let metadata = request.get_additional_metadata();
        let database = metadata.database.clone();
        let table = metadata.table.clone();

        let payload = request.take_payload();

        // 详细记录请求信息（无论log_request设置如何）
        info!(
            target: "doris_sink",
            "Building request for Doris stream load: database={}, table={}, payload_size={}, url_base={}", 
            database,
            table,
            payload.len(),
            self.url
        );

        // Generate a unique label
        let label = format!(
            "{}_{}_{}_{}_{}",
            self.label_prefix,
            database,
            table,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            Uuid::new_v4()
        );

        // Construct the stream load URL
        let stream_load_url = format!(
            "{}/api/{}/{}/_stream_load",
            self.url, database, table
        );

        debug!(
            target: "doris_sink",
            "Final Doris stream load URL: {}", 
            stream_load_url
        );

        // 创建请求构建器 - 确保使用PUT方法
        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(&stream_load_url)
            .header(CONTENT_LENGTH, payload.len())
            .header(CONTENT_TYPE, "text/plain;charset=utf-8")
            .header("Expect", "100-continue");

        // 添加所有headers
        let mut group_commit = false;
        debug!(target: "doris_sink", "Adding headers:");
        for (key, value) in &self.headers {
            debug!(target: "doris_sink", "  Header: {}={}", key, value);
            builder = builder.header(key, value);

            // 检查是否启用了group_commit
            if key == "group_commit" && value != "off_mode" {
                group_commit = true;
            }
        }

        // 只有在不使用group_commit时添加label
        if !group_commit {
            debug!(target: "doris_sink", "  Header: label={}", label);
            builder = builder.header("label", &label);
        }

        let auth: Option<Auth> = self.auth.clone();
        if let Some(auth) = auth {
            debug!(target: "doris_sink", "Applying authentication");
            builder = auth.apply_builder(builder);
        } else {
            debug!(target: "doris_sink", "No authentication provided");
        }

        // 记录完整请求 URL（如果开启了日志）
        if self.log_request {
            debug!(
                target: "doris_sink",
                "Doris stream load URL: {}", 
                stream_load_url
            );
        }

        // 构建请求
        builder
            .body(payload)
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}