use crate::{
    internal_events::{RedisEventReceived, RedisReceiveEventFailed},
    shutdown::ShutdownSignal,
    sources::redis::{create_event, Method},
    sources::Source,
    SourceSender,
};
use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};

pub fn watch(
    client: redis::Client,
    key: String,
    redis_key: Option<String>,
    method: Method,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Source {
    let fut = async move {
        trace!("Get redis connection manager.");
        let mut conn = client
            .get_tokio_connection_manager()
            .await
            .map_err(|_| ())?;
        trace!("Get redis connection manager success.");
        match method {
            Method::Brpop => loop {
                tokio::select! {
                    res = brpop(&mut conn,key.as_str()) => {
                        match res {
                            Ok(line) => {
                                emit!(&RedisEventReceived {
                                    byte_size: line.len()
                                });
                                let event = create_event(line.as_str(),key.clone(),&redis_key);
                                tokio::select! {
                                    result = out.send(event) => {match result {
                                        Ok(()) => { },
                                        Err(err) => error!(message = "Error sending event.", error = %err),
                                    }}
                                    _ = &mut shutdown => return Ok(()),
                                }
                            }
                            Err(error) => {
                                error!(message = "Redis source generated an error.", %error);
                                emit!(&RedisReceiveEventFailed { error });
                            }
                        }
                    }
                    _ = &mut shutdown => return Ok(()),
                }
            },
            Method::Blpop => loop {
                tokio::select! {
                    res = blpop(&mut conn,key.as_str()) => {
                        match res {
                            Ok(line) => {
                                emit!(&RedisEventReceived {
                                    byte_size: line.len()
                                });
                                let event = create_event(line.as_str(),key.clone(),&redis_key);
                                tokio::select! {
                                    result = out.send(event) => {match result {
                                        Ok(()) => { },
                                        Err(err) => error!(message = "Error sending event.", error = %err),
                                    }}
                                    _ = &mut shutdown => return Ok(()),
                                }
                            }
                            Err(error) => {
                                emit!(&RedisReceiveEventFailed {error});
                            }
                        }
                    }
                    _ = &mut shutdown => return Ok(()),
                }
            },
        }
    };
    Box::pin(fut)
}

async fn brpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    let res: RedisResult<(String, String)> = conn.brpop(key, 0).await;
    res.map(|(_, value)| value)
}

async fn blpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    let res: RedisResult<(String, String)> = conn.blpop(key, 0).await;
    res.map(|(_, value)| value)
}
