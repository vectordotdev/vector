//! Service implementation for the `Clickhouse` sink.

use bytes::Bytes;
use http::{
    Request, StatusCode, Uri,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
};
use snafu::ResultExt;

use super::{config::QuerySettingsConfig, sink::PartitionKey};
use crate::{
    http::{Auth, HttpError},
    sinks::{
        HTTPRequestBuilderSnafu, UriParseSnafu,
        clickhouse::config::Format,
        prelude::*,
        util::{
            http::{HttpRequest, HttpResponse, HttpRetryLogic, HttpServiceRequestBuilder},
            retries::RetryAction,
        },
    },
};

#[derive(Debug, Default, Clone)]
pub struct ClickhouseRetryLogic {
    inner: HttpRetryLogic<HttpRequest<PartitionKey>>,
}

impl RetryLogic for ClickhouseRetryLogic {
    type Error = HttpError;
    type Request = HttpRequest<PartitionKey>;
    type Response = HttpResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        match response.http_response.status() {
            StatusCode::INTERNAL_SERVER_ERROR => {
                let body = response.http_response.body();

                // Currently, ClickHouse returns 500's incorrect data and type mismatch errors.
                // This attempts to check if the body starts with `Code: {code_num}` and to not
                // retry those errors.
                //
                // Reference: https://github.com/vectordotdev/vector/pull/693#issuecomment-517332654
                // Error code definitions: https://github.com/ClickHouse/ClickHouse/blob/master/dbms/src/Common/ErrorCodes.cpp
                //
                // Fix already merged: https://github.com/ClickHouse/ClickHouse/pull/6271
                if body.starts_with(b"Code: 117") {
                    RetryAction::DontRetry("incorrect data".into())
                } else if body.starts_with(b"Code: 53") {
                    RetryAction::DontRetry("type mismatch".into())
                } else {
                    RetryAction::Retry(String::from_utf8_lossy(body).to_string().into())
                }
            }
            _ => self.inner.should_retry_response(&response.http_response),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ClickhouseServiceRequestBuilder {
    pub(super) auth: Option<Auth>,
    pub(super) endpoint: Uri,
    pub(super) skip_unknown_fields: Option<bool>,
    pub(super) date_time_best_effort: bool,
    pub(super) insert_random_shard: bool,
    pub(super) compression: Compression,
    pub(super) query_settings: QuerySettingsConfig,
}

impl HttpServiceRequestBuilder<PartitionKey> for ClickhouseServiceRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<PartitionKey>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let metadata = request.get_additional_metadata();

        let uri = set_uri_query(
            &self.endpoint,
            &metadata.database,
            &metadata.table,
            metadata.format,
            self.skip_unknown_fields,
            self.date_time_best_effort,
            self.insert_random_shard,
            self.query_settings,
        )?;

        let auth: Option<Auth> = self.auth.clone();

        // Extract format before taking payload to avoid borrow checker issues
        let format = metadata.format;
        let payload = request.take_payload();

        // Set content type based on format
        let content_type = match format {
            Format::ArrowStream => "application/vnd.apache.arrow.stream",
            _ => "application/x-ndjson",
        };

        let mut builder = Request::post(&uri)
            .header(CONTENT_TYPE, content_type)
            .header(CONTENT_LENGTH, payload.len());
        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header(CONTENT_ENCODING, ce);
        }
        if let Some(auth) = auth {
            builder = auth.apply_builder(builder);
        }

        builder
            .body(payload)
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}

fn append_param<T: ToString>(uri: &mut String, key: &str, value: Option<T>) {
    if let Some(val) = value {
        uri.push_str(&format!("{}={}&", key, val.to_string()));
    }
}
fn append_param_bool(uri: &mut String, key: &str, value: Option<bool>) {
    if let Some(val) = value {
        uri.push_str(&format!("{}={}&", key, if val { 1 } else { 0 }));
    }
}

#[allow(clippy::too_many_arguments)]
fn set_uri_query(
    uri: &Uri,
    database: &str,
    table: &str,
    format: Format,
    skip_unknown: Option<bool>,
    date_time_best_effort: bool,
    insert_random_shard: bool,
    query_settings: QuerySettingsConfig,
) -> crate::Result<Uri> {
    // Use ClickHouse query parameters with the Identifier type (introduced in 21.12) so
    // the server handles identifier quoting — no client-side escaping required.
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "query",
            &format!(
                "INSERT INTO {{database:Identifier}}.{{table:Identifier}} FORMAT {}",
                format
            ),
        )
        .append_pair("param_database", database)
        .append_pair("param_table", table)
        .finish();

    let mut uri = uri.to_string();
    if !uri.ends_with('/') {
        uri.push('/');
    }

    uri.push_str("?input_format_import_nested_json=1&");
    append_param_bool(&mut uri, "input_format_skip_unknown_fields", skip_unknown);
    if date_time_best_effort {
        uri.push_str("date_time_input_format=best_effort&")
    }
    if insert_random_shard {
        uri.push_str("insert_distributed_one_random_shard=1&")
    }
    append_param_bool(
        &mut uri,
        "async_insert",
        query_settings.async_insert_settings.enabled,
    );
    append_param_bool(
        &mut uri,
        "wait_for_async_insert",
        query_settings.async_insert_settings.wait_for_processing,
    );
    append_param(
        &mut uri,
        "wait_for_async_insert_timeout",
        query_settings
            .async_insert_settings
            .wait_for_processing_timeout,
    );
    append_param_bool(
        &mut uri,
        "async_insert_deduplicate",
        query_settings.async_insert_settings.deduplicate,
    );
    append_param(
        &mut uri,
        "async_insert_max_data_size",
        query_settings.async_insert_settings.max_data_size,
    );
    append_param(
        &mut uri,
        "async_insert_max_query_number",
        query_settings.async_insert_settings.max_query_number,
    );
    uri.push_str(query.as_str());

    uri.parse::<Uri>()
        .context(UriParseSnafu)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::super::config::AsyncInsertSettingsConfig;
    use super::*;

    fn parse_query_params(uri: &Uri) -> std::collections::HashMap<String, String> {
        url::form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn encode_valid() {
        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            Format::JsonEachRow,
            Some(false),
            true,
            false,
            QuerySettingsConfig::default(),
        )
        .unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     input_format_skip_unknown_fields=0&\
                                     date_time_input_format=best_effort&\
                                     query=INSERT+INTO+%7Bdatabase%3AIdentifier%7D.%7Btable%3AIdentifier%7D+FORMAT+JSONEachRow&\
                                     param_database=my_database&\
                                     param_table=my_table"
        );

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonEachRow,
            Some(false),
            false,
            false,
            QuerySettingsConfig::default(),
        )
        .unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     input_format_skip_unknown_fields=0&\
                                     query=INSERT+INTO+%7Bdatabase%3AIdentifier%7D.%7Btable%3AIdentifier%7D+FORMAT+JSONEachRow&\
                                     param_database=my_database&\
                                     param_table=my_%22table%22"
        );

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonAsObject,
            Some(true),
            true,
            false,
            QuerySettingsConfig::default(),
        )
        .unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     input_format_skip_unknown_fields=1&\
                                     date_time_input_format=best_effort&\
                                     query=INSERT+INTO+%7Bdatabase%3AIdentifier%7D.%7Btable%3AIdentifier%7D+FORMAT+JSONAsObject&\
                                     param_database=my_database&\
                                     param_table=my_%22table%22"
        );

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonAsObject,
            None,
            true,
            false,
            QuerySettingsConfig::default(),
        )
        .unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     date_time_input_format=best_effort&\
                                     query=INSERT+INTO+%7Bdatabase%3AIdentifier%7D.%7Btable%3AIdentifier%7D+FORMAT+JSONAsObject&\
                                     param_database=my_database&\
                                     param_table=my_%22table%22"
        );

        let uri = set_uri_query(
            &"http://localhost:80".parse().unwrap(),
            "my_database",
            "my_\"table\"",
            Format::JsonAsObject,
            None,
            true,
            false,
            QuerySettingsConfig {
                async_insert_settings: AsyncInsertSettingsConfig {
                    enabled: Some(true),
                    wait_for_processing: Some(true),
                    wait_for_processing_timeout: Some(500),
                    ..AsyncInsertSettingsConfig::default()
                },
            },
        )
        .unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:80/?\
                                     input_format_import_nested_json=1&\
                                     date_time_input_format=best_effort&\
                                     async_insert=1&\
                                     wait_for_async_insert=1&\
                                     wait_for_async_insert_timeout=500&\
                                     query=INSERT+INTO+%7Bdatabase%3AIdentifier%7D.%7Btable%3AIdentifier%7D+FORMAT+JSONAsObject&\
                                     param_database=my_database&\
                                     param_table=my_%22table%22"
        );
    }

    #[test]
    fn identifier_params() {
        fn params(database: &str, table: &str) -> (String, String, String) {
            let uri = set_uri_query(
                &"http://localhost:80".parse().unwrap(),
                database,
                table,
                Format::JsonEachRow,
                None,
                false,
                false,
                QuerySettingsConfig::default(),
            )
            .unwrap();
            let p = parse_query_params(&uri);
            (
                p["query"].clone(),
                p["param_database"].clone(),
                p["param_table"].clone(),
            )
        }

        // The query template is always the same fixed string regardless of identifier content.
        let template = "INSERT INTO {database:Identifier}.{table:Identifier} FORMAT JSONEachRow";

        // Plain identifiers are passed through as-is.
        let (q, db, tbl) = params("my_db", "my_table");
        assert_eq!(q, template);
        assert_eq!(db, "my_db");
        assert_eq!(tbl, "my_table");

        // Special characters are passed as raw values; ClickHouse handles quoting.
        let (q, db, tbl) = params("my_db", r#"my_"table""#);
        assert_eq!(q, template);
        assert_eq!(db, "my_db");
        assert_eq!(tbl, r#"my_"table""#);

        // Injection payload: the database and table params are independent URL parameters,
        // so there is no SQL to break out of.
        let (q, db, tbl) = params(r#"valid_db"."other_table" --"#, "my_table");
        assert_eq!(q, template);
        assert_eq!(db, r#"valid_db"."other_table" --"#);
        assert_eq!(tbl, "my_table");

        // Backslash and quotes together.
        let (q, db, tbl) = params("db_with_\\\"", "my_table");
        assert_eq!(q, template);
        assert_eq!(db, "db_with_\\\"");
        assert_eq!(tbl, "my_table");
    }

    #[test]
    fn encode_invalid() {
        set_uri_query(
            &"localhost:80".parse().unwrap(),
            "my_database",
            "my_table",
            Format::JsonEachRow,
            Some(false),
            false,
            false,
            QuerySettingsConfig::default(),
        )
        .unwrap_err();
    }
}
