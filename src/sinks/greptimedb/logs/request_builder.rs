use greptimedb_client::{
    api::v1::{Row, RowInsertRequest, Rows},
    helpers::values::{string_value, timestamp_millisecond_value},
};
use vector_lib::event::LogEvent;
use vrl::core::Value::*;

use crate::sinks::greptimedb::{str_column, ts_column};

pub fn log_to_insert_request(log: LogEvent, table: String) -> RowInsertRequest {
    let mut schema = Vec::new();
    let mut columns = Vec::new();

    let v = log.value();
    let m = match v {
        Object(map) => map,
        _ => unreachable!(),
    };

    for (k, v) in m {
        if k.as_str() == "timestamp" {
            schema.push(ts_column("timestamp"));
            let ts = match v {
                Timestamp(ts) => ts,
                _ => unreachable!(),
            };
            columns.push(timestamp_millisecond_value(ts.timestamp_millis()));
        } else {
            schema.push(str_column(k.as_str()));

            let b = match v {
                Bytes(b) => b,
                _ => unreachable!(),
            };

            let s = std::str::from_utf8(b).unwrap().to_string();
            columns.push(string_value(s));
        }
    }

    RowInsertRequest {
        table_name: table,
        rows: Some(Rows {
            schema,
            rows: vec![Row { values: columns }],
        }),
    }
}
