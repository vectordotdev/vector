use crate::sinks::util::BoxedRawValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertAllRequestRows {
    // [Optional] A unique ID for each row. BigQuery uses this property to detect duplicate
    // insertion requests on a best-effort basis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_id: Option<String>,
    // Represents a single JSON object.
    pub json: BoxedRawValue,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertAllRequest {
    // [Optional] Accept rows that contain values that do not match the schema. The unknown values
    // are ignored. Default is false, which treats unknown values as errors.
    pub ignore_unknown_values: bool,
    // The resource type of the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    // The rows to insert.
    pub rows: Vec<InsertAllRequestRows>,
    // [Optional] Insert all valid rows of a request, even if invalid rows exist. The default value
    // is false, which causes the entire request to fail if any invalid rows exist.
    pub skip_invalid_rows: bool,
    // If specified, treats the destination table as a base template, and inserts the rows into an
    // instance table named \"{destination}{templateSuffix}\". BigQuery will manage creation of
    // the instance table, using the schema of the base template table. See https://cloud.google.com
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_suffix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertAllResponse {
    // An array of errors for rows that were not inserted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_errors: Option<Vec<InsertErrors>>,
    // The resource type of the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

const MAX_ERROR_MESSAGES: usize = 10;
impl InsertAllResponse {
    pub fn get_error_messages(&self) -> String {
        let mut messages = vec![];
        // iterate over errors, look for "no such field" error message
        if let Some(insert_all_errors) = &self.insert_errors {
            for insert_all_error in insert_all_errors {
                if let Some(row_errors) = &insert_all_error.errors {
                    for row_error in row_errors {
                        if let Some(msg) = &row_error.message {
                            messages.push(msg.clone());
                            if messages.len() >= MAX_ERROR_MESSAGES {
                                return messages.join(",");
                            }
                        }
                    }
                }
            }
        }
        messages.join(",")
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertErrors {
    // Error information for the row indicated by the index property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ErrorProto>>,
    // The index of the row that error applies to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorProto {
    // Debugging information. This property is internal to Google and should not be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<String>,
    // Specifies where the error occurred, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    // A human-readable description of the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    // A short error code that summarizes the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
