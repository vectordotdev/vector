use greptimedb_client::{
    api::v1::{ColumnDataType, ColumnSchema, Row, RowInsertRequest, Rows, SemanticType},
    helpers::values::{string_value, timestamp_millisecond_value},
};
use vector_lib::event::LogEvent;
use vrl::core::Value::*;

pub fn log_to_insert_request(log: LogEvent, table: String) -> RowInsertRequest {
    let mut schema = Vec::new();
    let mut columns = Vec::new();
    // warn!("{:?}", log);

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

        warn!("{:?} {:?}", k, v);
    }

    RowInsertRequest {
        table_name: table,
        rows: Some(Rows {
            schema,
            rows: vec![Row { values: columns }],
        }),
    }
}

fn ts_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Timestamp as i32,
        datatype: ColumnDataType::TimestampMillisecond as i32,
        ..Default::default()
    }
}

fn str_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Field as i32,
        datatype: ColumnDataType::String as i32,
        ..Default::default()
    }
}
