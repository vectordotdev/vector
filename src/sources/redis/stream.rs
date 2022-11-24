use std::str::from_utf8;

use crate::{
    codecs,
    config::SourceContext,
    internal_events::RedisReceiveEventError,
    sources::{redis::handle_line, Source},
};
use redis::{aio::Connection, streams::*, AsyncCommands, RedisError, RedisResult, Value};
use snafu::{ResultExt, Snafu};
use vector_common::internal_event::{BytesReceived, Registered};
use vector_core::config::LogNamespace;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: RedisError },
}

pub struct StreamInputs {
    pub client: redis::Client,
    pub bytes_received: Registered<BytesReceived>,
    pub key: String,
    pub redis_key: Option<String>,
    pub decoder: codecs::Decoder,
    pub cx: SourceContext,
    pub log_namespace: LogNamespace,
}

fn redis_value_to_string(value: &Value) -> String {
    let mut result_string = String::new();

    match value {
        redis::Value::Nil => {}
        redis::Value::Int(number) => result_string.push_str(&number.to_string()),
        redis::Value::Data(byte_array) => {
            result_string.push_str(&from_utf8(byte_array).unwrap().to_string())
        }
        redis::Value::Bulk(values) => {
            for val in values.iter() {
                result_string.push_str(redis_value_to_string(val).as_str());
                result_string.push(' ');
            }
        }
        redis::Value::Status(status) => result_string.push_str(status),
        redis::Value::Okay => result_string.push_str("OK"),
    }

    result_string
}

fn stream_id_to_string(stream_id: StreamId) -> String {
    let mut result_string = String::new();

    for (key, value) in stream_id.map.iter() {
        result_string.push_str(key);
        result_string.push(' ');
        result_string.push_str(redis_value_to_string(value).as_str());
        result_string.push(' ');
    }

    result_string
}

async fn read_stream(mut conn: Connection, key: String) -> RedisResult<String> {
    let opts = StreamReadOptions::default().count(1).block(0);
    let result: Option<StreamReadReply> = conn.xread_options(&[key], &["$"], &opts).await.unwrap();
    let mut line: String = "".to_owned();
    if let Some(reply) = result {
        for stream_key in reply.keys {
            for stream_id in stream_key.ids {
                line.push_str(&stream_id_to_string(stream_id));
            }
        }
    }

    Ok(line)
}

pub async fn read(input: StreamInputs) -> crate::Result<Source> {
    Ok(Box::pin(async move {
        let mut tx = input.cx.out;
        let mut shutdown = input.cx.shutdown;
        Ok(loop {
            let conn = input
                .client
                .get_async_connection()
                .await
                .context(ConnectionSnafu {})
                .unwrap();
            let res = tokio::select! {
                res = read_stream(conn, input.key.clone()) => res,
                _ = &mut shutdown => break
            };
            match res {
                Err(error) => emit!(RedisReceiveEventError::from(error)),
                Ok(line) => {
                    if let Err(()) = handle_line(
                        line,
                        &input.key,
                        input.redis_key.as_deref(),
                        input.decoder.clone(),
                        &input.bytes_received,
                        &mut tx,
                        input.log_namespace,
                    )
                    .await
                    {
                        break;
                    }
                }
            }
        })
    }))
}
