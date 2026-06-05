use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use chrono::Utc;
use databend_client::{APIClient as DatabendAPIClient, Error as DatabendError};
use futures::future::BoxFuture;
use rand::{Rng, rng};
use rand_distr::Alphanumeric;
use snafu::Snafu;
use tower::Service;
use vector_lib::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{internal_events::EndpointBytesSent, sinks::util::retries::RetryLogic};

use super::config::DatabendLoadMode;

fn quote_identifier(identifier: &str) -> String {
    format!("`{}`", identifier.replace('`', "``"))
}

fn quote_sql_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for char in value.chars() {
        match char {
            '\'' => quoted.push_str("''"),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            char => quoted.push(char),
        }
    }
    quoted.push('\'');
    quoted
}

fn file_format_option_sql(key: &str, value: &str) -> String {
    let value = match key {
        "field_delimiter" | "record_delimiter" => quote_sql_string(value),
        _ => value.to_string(),
    };

    format!("{key}={value}")
}

#[derive(Clone)]
pub struct DatabendRetryLogic;

impl RetryLogic for DatabendRetryLogic {
    type Error = DatabendError;
    type Request = DatabendRequest;
    type Response = DatabendResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            DatabendError::Response { status, .. } => {
                match status.as_u16() {
                    429 => true,
                    // general server error
                    500 => true,
                    // storage doesn't support presign operation
                    3902 => false,
                    // fail to parse stage attachment
                    1046 => false,
                    _ => false,
                }
            }
            DatabendError::WithContext { inner, .. } => self.is_retriable_error(inner),
            DatabendError::IO(_) => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct DatabendService {
    client: Arc<DatabendAPIClient>,
    table: String,
    load_mode: DatabendLoadMode,
    stage: String,
    stage_path_prefix: String,
    file_format_options: BTreeMap<&'static str, &'static str>,
    copy_options: BTreeMap<&'static str, &'static str>,
    primary_key: Vec<String>,
}

pub struct DatabendServiceSettings {
    pub table: String,
    pub load_mode: DatabendLoadMode,
    pub stage: String,
    pub stage_path_prefix: String,
    pub file_format_options: BTreeMap<&'static str, &'static str>,
    pub copy_options: BTreeMap<&'static str, &'static str>,
    pub primary_key: Vec<String>,
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
        client: Arc<DatabendAPIClient>,
        settings: DatabendServiceSettings,
    ) -> Result<Self, DatabendError> {
        let DatabendServiceSettings {
            table,
            load_mode,
            stage,
            stage_path_prefix,
            file_format_options,
            copy_options,
            primary_key,
        } = settings;

        if table.is_empty() {
            return Err(DatabendError::BadArgument("table is required".to_string()));
        }
        if matches!(load_mode, DatabendLoadMode::Streaming) && !primary_key.is_empty() {
            return Err(DatabendError::BadArgument(
                "primary_key is not supported with load_mode = streaming".to_string(),
            ));
        }
        Ok(Self {
            client,
            table,
            load_mode,
            stage,
            stage_path_prefix,
            file_format_options,
            copy_options,
            primary_key,
        })
    }

    async fn new_stage_location(&self) -> String {
        let now = Utc::now().timestamp();
        let database = self
            .client
            .current_database()
            .unwrap_or("default".to_string());
        let suffix = rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .collect::<String>();
        let stage = self.stage.trim_start_matches('@');
        let prefix = self.stage_path_prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("@{}/{}/{}/{}-{}", stage, database, self.table, now, suffix,)
        } else {
            format!(
                "@{}/{}/{}/{}/{}-{}",
                stage, prefix, database, self.table, now, suffix,
            )
        }
    }

    pub(crate) async fn load(&self, data: Bytes) -> Result<(), DatabendError> {
        match (self.load_mode, self.primary_key.is_empty()) {
            (DatabendLoadMode::Staged, true) => self.insert_with_stage(data).await,
            (DatabendLoadMode::Staged, false) => self.replace_with_stage(data).await,
            (DatabendLoadMode::Streaming, true) => self.streaming_load(data).await,
            (DatabendLoadMode::Streaming, false) => {
                unreachable!("validated in DatabendService::new")
            }
        }
    }

    async fn insert_with_stage(&self, data: Bytes) -> Result<(), DatabendError> {
        let stage = self.new_stage_location().await;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));
        self.client.upload_to_stage(&stage, reader, size).await?;
        let table = quote_identifier(&self.table);
        let sql = format!("INSERT INTO {table} VALUES");
        let _ = self
            .client
            .insert_with_stage(
                &sql,
                &stage,
                self.file_format_options.clone(),
                self.copy_options.clone(),
            )
            .await?;
        Ok(())
    }

    async fn streaming_load(&self, data: Bytes) -> Result<(), DatabendError> {
        let reader = Box::new(Cursor::new(data));
        let table = quote_identifier(&self.table);
        let sql = format!(
            "INSERT INTO {table} FROM @_databend_load FILE_FORMAT=({})",
            self.file_format_options_sql()
        );
        let _ = self
            .client
            .streaming_load(
                &sql,
                reader,
                &format!("vector-batch.{}", self.file_extension()),
            )
            .await?;
        Ok(())
    }

    async fn replace_with_stage(&self, data: Bytes) -> Result<(), DatabendError> {
        let stage = self.new_stage_location().await;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));
        self.client.upload_to_stage(&stage, reader, size).await?;
        let table = quote_identifier(&self.table);
        let primary_key = self
            .primary_key
            .iter()
            .map(|key| quote_identifier(key))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("REPLACE INTO {table} ON ({primary_key}) VALUES");
        let _ = self
            .client
            .insert_with_stage(
                &sql,
                &stage,
                self.file_format_options.clone(),
                self.copy_options.clone(),
            )
            .await?;
        Ok(())
    }

    fn file_extension(&self) -> &'static str {
        match self.file_format_options.get("type").copied() {
            Some("CSV") => "csv",
            _ => "ndjson",
        }
    }

    fn file_format_options_sql(&self) -> String {
        self.file_format_options
            .iter()
            .map(|(key, value)| file_format_option_sql(key, value))
            .collect::<Vec<_>>()
            .join(" ")
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
            let protocol = service.client.scheme();
            let host_port = format!("{}:{}", service.client.host(), service.client.port());
            let endpoint = host_port.as_str();
            let byte_size = request.data.len();
            service.load(request.data).await?;
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{file_format_option_sql, quote_identifier, quote_sql_string};

    #[test]
    fn quote_identifier_escapes_backticks() {
        assert_eq!(quote_identifier("events"), "`events`");
        assert_eq!(quote_identifier("odd`table"), "`odd``table`");
    }

    #[test]
    fn quote_sql_string_escapes_special_characters() {
        assert_eq!(quote_sql_string(","), "','");
        assert_eq!(quote_sql_string("\n"), "'\\n'");
        assert_eq!(quote_sql_string("it's"), "'it''s'");
    }

    #[test]
    fn file_format_option_sql_quotes_string_values() {
        assert_eq!(file_format_option_sql("type", "CSV"), "type=CSV");
        assert_eq!(
            file_format_option_sql("field_delimiter", ","),
            "field_delimiter=','"
        );
        assert_eq!(
            file_format_option_sql("record_delimiter", "\n"),
            "record_delimiter='\\n'"
        );
        assert_eq!(
            file_format_option_sql("missing_field_as", "NULL"),
            "missing_field_as=NULL"
        );
        assert_eq!(file_format_option_sql("skip_header", "0"), "skip_header=0");
    }

    #[test]
    fn file_format_options_are_stable_for_streaming_sql() {
        let options = BTreeMap::from([
            ("compression", "GZIP"),
            ("field_delimiter", ","),
            ("missing_field_as", "FIELD_DEFAULT"),
            ("record_delimiter", "\n"),
            ("skip_header", "0"),
            ("type", "CSV"),
        ]);

        let sql = options
            .iter()
            .map(|(key, value)| file_format_option_sql(key, value))
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(
            sql,
            "compression=GZIP field_delimiter=',' missing_field_as=FIELD_DEFAULT record_delimiter='\\n' skip_header=0 type=CSV"
        );
    }
}
