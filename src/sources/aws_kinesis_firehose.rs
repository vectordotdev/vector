use crate::{
    config::{DataType, GlobalOptions, SinkDescription, SourceConfig},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use async_trait::async_trait;
use futures::{compat::Future01CompatExt, FutureExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AwsKinesisFirehoseConfig {
    address: SocketAddr,
    access_key: Option<String>,
    tls: Option<TlsConfig>,
}

#[typetag::serde(name = "aws_kinesis_firehose")]
#[async_trait]
impl SourceConfig for AwsKinesisFirehoseConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        _shutdown: ShutdownSignal,
        _out: Pipeline,
    ) -> crate::Result<super::Source> {
        unimplemented!()
    }

    async fn build_async(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let svc = filters::firehose(self.access_key.clone(), out);

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let mut listener = tls.bind(&self.address).await?;

        let fut = async move {
            let _ = warp::serve(svc)
                .serve_incoming_with_graceful_shutdown(
                    listener.incoming(),
                    shutdown.clone().compat().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        };
        Ok(Box::new(fut.boxed().compat()))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }
}

inventory::submit! {
    SinkDescription::new_without_default::<AwsKinesisFirehoseConfig>("aws_kinesis_firehose")
}

mod filters {
    use super::{
        errors::{Parse, RequestError},
        handlers,
        models::{FirehoseRequest, FirehoseResponse},
    };
    use crate::{
        internal_events::{AwsKinesisFirehoseRequestError, AwsKinesisFirehoseRequestReceived},
        Pipeline,
    };
    use bytes::{buf::BufExt, Bytes};
    use chrono::Utc;
    use flate2::read::GzDecoder;
    use snafu::ResultExt;
    use std::convert::Infallible;
    use std::io;
    use warp::http::StatusCode;
    use warp::Filter;

    /// Handles routing of incoming HTTP requests from AWS Kinesis Firehose
    pub fn firehose(
        access_key: Option<String>,
        out: Pipeline,
    ) -> impl Filter<Extract = impl warp::Reply, Error = Infallible> + Clone {
        warp::post()
            .and(emit_received())
            .and(authenticate(access_key))
            .and(warp::header("X-Amz-Firehose-Request-Id"))
            .and(warp::header("X-Amz-Firehose-Source-Arn"))
            .and(warp::header::exact(
                "X-Amz-Firehose-Protocol-Version",
                "1.0",
            ))
            .and(parse_body())
            .and(warp::any().map(move || out.clone()))
            .and_then(handlers::firehose)
            .recover(handle_firehose_rejection)
    }

    /// Decode (if needed) and parse request body
    ///
    /// Firehose can be configured to gzip compress messages so we handle this here
    fn parse_body(
    ) -> impl Filter<Extract = (FirehoseRequest,), Error = warp::reject::Rejection> + Clone {
        warp::any()
            .and(warp::header::optional::<String>("Content-Encoding"))
            .and(warp::header("X-Amz-Firehose-Request-Id"))
            .and(warp::body::bytes())
            .and_then(
                |encoding: Option<String>, request_id: String, body: Bytes| async move {
                    match encoding {
                        Some(s) if s.as_bytes() == b"gzip" => {
                            Ok(Box::new(GzDecoder::new(body.reader())) as Box<dyn io::Read>)
                        }
                        Some(s) => Err(warp::reject::Rejection::from(
                            RequestError::UnsupportedEncoding {
                                encoding: s,
                                request_id: request_id.clone(),
                            },
                        )),
                        None => Ok(Box::new(body.reader()) as Box<dyn io::Read>),
                    }
                    .and_then(|r| {
                        serde_json::from_reader(r)
                            .context(Parse {
                                request_id: request_id.clone(),
                            })
                            .map_err(|e| warp::reject::custom(e))
                    })
                },
            )
    }

    fn emit_received() -> impl Filter<Extract = (), Error = warp::reject::Rejection> + Clone {
        warp::any()
            .and(warp::header::optional("X-Amz-Firehose-Request-Id"))
            .and(warp::header::optional("X-Amz-Firehose-Source-Arn"))
            .map(|request_id: Option<String>, source_arn: Option<String>| {
                emit!(AwsKinesisFirehoseRequestReceived {
                    request_id: request_id.as_deref(),
                    source_arn: source_arn.as_deref(),
                });
            })
            .untuple_one()
    }

    /// If there is a configured access key, validate that the request key matches it
    fn authenticate(
        configured_access_key: Option<String>,
    ) -> impl Filter<Extract = (), Error = warp::Rejection> + Clone {
        warp::any()
            .and(warp::header("X-Amz-Firehose-Request-Id"))
            .and(warp::header::optional("X-Amz-Firehose-Access-Key"))
            .and_then(move |request_id: String, access_key: Option<String>| {
                let configured_access_key = configured_access_key.clone();
                async move {
                    match (access_key, configured_access_key) {
                        (_, None) => Ok(()),
                        (Some(configured_access_key), Some(access_key))
                            if configured_access_key == access_key =>
                        {
                            Ok(())
                        }
                        (Some(_), Some(_)) => {
                            Err(warp::reject::custom(RequestError::AccessKeyInvalid {
                                request_id,
                            }))
                        }
                        (None, Some(_)) => {
                            Err(warp::reject::custom(RequestError::AccessKeyMissing {
                                request_id,
                            }))
                        }
                    }
                }
            })
            .untuple_one()
    }

    /// Maps RequestError and warp errors to AWS Kinesis Firehose response structure
    async fn handle_firehose_rejection(
        err: warp::Rejection,
    ) -> Result<impl warp::Reply, Infallible> {
        let request_id: Option<String>;
        let message: String;
        let code: StatusCode;

        if let Some(e) = err.find::<RequestError>() {
            message = format!("{}", e);
            code = e.status();
            request_id = Some(e.request_id());
        } else {
            code = StatusCode::INTERNAL_SERVER_ERROR;
            message = format!("{:?}", err);
            request_id = None;
        }

        emit!(AwsKinesisFirehoseRequestError {
            request_id: request_id.as_deref(),
            error: message.as_str(),
        });

        let json = warp::reply::json(&FirehoseResponse {
            request_id: request_id.unwrap_or_default(),
            timestamp: Utc::now(),
            error_message: Some(message.clone()),
        });

        Ok(warp::reply::with_status(json, code))
    }
}

mod handlers {
    use super::errors::{ParseRecords, RequestError};
    use super::models::{EncodedFirehoseRecord, FirehoseRequest, FirehoseResponse};
    use crate::{config::log_schema, event::Event, Pipeline};
    use chrono::Utc;
    use flate2::read::GzDecoder;
    use futures::{compat::Future01CompatExt, TryFutureExt};
    use futures01::Sink;
    use snafu::ResultExt;
    use std::io::Read;
    use warp::reject;

    /// Publishes decoded events from the FirehoseRequest to the pipeline
    pub async fn firehose(
        request_id: String,
        source_arn: String,
        request: FirehoseRequest,
        out: Pipeline,
    ) -> Result<impl warp::Reply, reject::Rejection> {
        match parse_records(request, request_id.as_str(), source_arn.as_str()).with_context(|| {
            ParseRecords {
                request_id: request_id.clone(),
            }
        }) {
            Ok(events) => {
                let request_id = request_id.clone();
                out.send_all(futures01::stream::iter_ok(events))
                    .compat()
                    .map_err(|err| {
                        let err = RequestError::ShuttingDown {
                            request_id: request_id.clone(),
                            source: err,
                        };
                        // can only fail if receiving end disconnected, so we are shutting down,
                        // probably not gracefully.
                        error!("Failed to forward events, downstream is closed");
                        error!("Tried to send the following event: {:?}", err);
                        warp::reject::custom(err)
                    })
                    .map_ok(|_| {
                        warp::reply::json(&FirehoseResponse {
                            request_id: request_id.clone(),
                            timestamp: Utc::now(),
                            error_message: None,
                        })
                    })
                    .await
            }
            Err(err) => Err(reject::custom(err)),
        }
    }

    /// Parses out events from the FirehoseRequest
    fn parse_records(
        request: FirehoseRequest,
        request_id: &str,
        source_arn: &str,
    ) -> std::io::Result<Vec<Event>> {
        let records: Vec<Event> = request
            .records
            .iter()
            .map(|record| {
                decode_record(record).map(|record| {
                    let mut event = Event::new_empty_log();
                    let log = event.as_mut_log();

                    log.insert(log_schema().message_key().clone(), record);
                    log.insert(log_schema().timestamp_key().clone(), request.timestamp);
                    log.insert("request_id", request_id.to_string());
                    log.insert("source_arn", source_arn.to_string());

                    event
                })
            })
            .collect::<std::io::Result<Vec<Event>>>()?;

        Ok(records)
    }

    /// Decodes a Firehose record from its base64 gzip format
    fn decode_record(record: &EncodedFirehoseRecord) -> std::io::Result<Vec<u8>> {
        let mut cursor = std::io::Cursor::new(record.data.as_bytes());
        let base64decoder = base64::read::DecoderReader::new(&mut cursor, base64::STANDARD);

        let mut gz = GzDecoder::new(base64decoder);
        let mut buffer = Vec::new();
        gz.read_to_end(&mut buffer)?;

        Ok(buffer)
    }
}

mod models {
    use chrono::serde::ts_milliseconds;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    /// Represents an AWS Kinesis Firehose request
    ///
    /// Represents protocol v1.0 (the only protocol as of writing)
    ///
    /// https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FirehoseRequest {
        pub request_id: String,

        #[serde(with = "ts_milliseconds")]
        pub timestamp: DateTime<Utc>,

        pub records: Vec<EncodedFirehoseRecord>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct EncodedFirehoseRecord {
        /// data is base64 encoded, gzip'd, bytes
        pub data: String,
    }

    /// Represents an AWS Kinesis Firehose response
    ///
    /// Represents protocol v1.0 (the only protocol as of writing)
    ///
    /// https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FirehoseResponse {
        pub request_id: String,

        #[serde(with = "ts_milliseconds")]
        pub timestamp: DateTime<Utc>,

        pub error_message: Option<String>,
    }
}

pub mod errors {
    use crate::event::Event;
    use snafu::Snafu;
    use warp::http::StatusCode;

    #[derive(Debug, Snafu)]
    #[snafu(visibility = "pub")]
    pub enum RequestError {
        #[snafu(display("X-Amz-Firehose-Access-Key required for request: {}", request_id))]
        AccessKeyMissing { request_id: String },
        #[snafu(display(
            "X-Amz-Firehose-Access-Key does not match configured key for request: {}",
            request_id
        ))]
        AccessKeyInvalid { request_id: String },
        #[snafu(display("Could not parse incoming request {}: {}", request_id, source))]
        Parse {
            source: serde_json::error::Error,
            request_id: String,
        },
        #[snafu(display(
            "Could not parse records from incoming request {}: {}",
            request_id,
            source
        ))]
        ParseRecords {
            source: std::io::Error,
            request_id: String,
        },
        #[snafu(display("Could not decode record for request {}: {}", request_id, source))]
        Decode {
            source: std::io::Error,
            request_id: String,
        },
        #[snafu(display(
            "Could not forward events for request {}, downstream is closed: {}",
            request_id,
            source
        ))]
        ShuttingDown {
            source: futures01::sync::mpsc::SendError<Event>,
            request_id: String,
        },
        #[snafu(display("Unsupported encoding: {}", encoding))]
        UnsupportedEncoding {
            encoding: String,
            request_id: String,
        },
    }

    impl warp::reject::Reject for RequestError {}

    impl RequestError {
        pub fn status(&self) -> StatusCode {
            match *self {
                RequestError::AccessKeyMissing { .. } => StatusCode::UNAUTHORIZED,
                RequestError::AccessKeyInvalid { .. } => StatusCode::UNAUTHORIZED,
                RequestError::Parse { .. } => StatusCode::UNAUTHORIZED,
                RequestError::UnsupportedEncoding { .. } => StatusCode::BAD_REQUEST,
                RequestError::ParseRecords { .. } => StatusCode::BAD_REQUEST,
                RequestError::Decode { .. } => StatusCode::BAD_REQUEST,
                RequestError::ShuttingDown { .. } => StatusCode::SERVICE_UNAVAILABLE,
            }
        }

        pub fn request_id(&self) -> String {
            match *self {
                RequestError::AccessKeyMissing { ref request_id, .. } => request_id,
                RequestError::AccessKeyInvalid { ref request_id, .. } => request_id,
                RequestError::Parse { ref request_id, .. } => request_id,
                RequestError::UnsupportedEncoding { ref request_id, .. } => request_id,
                RequestError::ParseRecords { ref request_id, .. } => request_id,
                RequestError::Decode { ref request_id, .. } => request_id,
                RequestError::ShuttingDown { ref request_id, .. } => request_id,
            }
            .clone()
        }
    }

    impl From<RequestError> for warp::reject::Rejection {
        fn from(error: RequestError) -> Self {
            warp::reject::custom(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shutdown::ShutdownSignal;
    use crate::{
        config::{GlobalOptions, SourceConfig},
        event::{Event, LogEvent},
        log_event,
        test_util::{collect_ready, next_addr, wait_for_tcp},
        Pipeline,
    };
    use chrono::{DateTime, SubsecRound, Utc};
    use flate2::{read::GzEncoder, Compression};
    use futures::compat::Future01CompatExt;
    use futures01::sync::mpsc;
    use pretty_assertions::assert_eq;
    use std::io::{Cursor, Read};
    use std::net::SocketAddr;

    async fn source(access_key: Option<String>) -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr();
        tokio::spawn(async move {
            AwsKinesisFirehoseConfig {
                address,
                tls: None,
                access_key,
            }
            .build_async(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .await
            .unwrap()
            .compat()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    /// Sends the body to the address with the appropriate Firehose headers
    ///
    /// https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html
    async fn send(
        address: SocketAddr,
        timestamp: DateTime<Utc>,
        records: Vec<&str>,
        key: Option<&str>,
        request_id: &str,
        source_arn: &str,
        gzip: bool,
    ) -> reqwest::Result<reqwest::Response> {
        let request = models::FirehoseRequest {
            request_id: request_id.to_string(),
            timestamp,
            records: records
                .into_iter()
                .map(|record| models::EncodedFirehoseRecord {
                    data: encode_record(record).unwrap().to_string(),
                })
                .collect(),
        };

        let mut builder = reqwest::Client::new()
            .post(&format!("http://{}", address))
            .header("host", address.to_string())
            .header(
                "x-amzn-trace-id",
                "Root=1-5f5fbf1c-877c68cace58bea222ddbeec",
            )
            .header("x-amz-firehose-protocol-version", "1.0")
            .header("x-amz-firehose-request-id", request_id.to_string())
            .header("x-amz-firehose-source-arn", source_arn.to_string())
            .header("user-agent", "Amazon Kinesis Data Firehose Agent/1.0")
            .header("content-type", "application/json");

        if let Some(key) = key {
            builder = builder.header("x-amz-firehose-access-key", key);
        }

        if gzip {
            let mut gz = GzEncoder::new(
                Cursor::new(serde_json::to_vec(&request).unwrap()),
                Compression::fast(),
            );
            let mut buffer = Vec::new();
            gz.read_to_end(&mut buffer).unwrap();
            builder = builder.header("content-encoding", "gzip").body(buffer);
        } else {
            builder = builder.json(&request);
        }

        builder.send().await
    }

    /// Encodes record data to mach AWS's representation: base64 encoded, gzip'd data
    fn encode_record(record: &str) -> std::io::Result<String> {
        let mut buffer = Vec::new();

        let mut gz = GzEncoder::new(record.as_bytes(), Compression::fast());
        gz.read_to_end(&mut buffer)?;

        Ok(base64::encode(&buffer))
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events() {
        // example CloudWatch Logs subscription event
        let record = r#"
{
  "messageType": "DATA_MESSAGE",
  "owner": "071959437513",
  "logGroup": "/jesse/test",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
  "logEvents": [
    {
      "id": "35683658089614582423604394983260738922885519999578275840",
      "timestamp": 1600110569039,
      "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
    },
    {
      "id": "35683658089659183914001456229543810359430816722590236673",
      "timestamp": 1600110569041,
      "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
    }
  ]
}
"#;

        let (rx, addr) = source(None).await;

        let source_arn = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";
        let timestamp: DateTime<Utc> = Utc::now();

        let res = send(
            addr,
            timestamp,
            vec![record],
            None,
            request_id,
            source_arn,
            false,
        )
        .await
        .unwrap();
        assert_eq!(200, res.status().as_u16());

        let events = collect_ready(rx).await.unwrap();
        assert_eq!(
            events,
            vec![log_event! {
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> record,
                "request_id" => request_id,
                "source_arn" => source_arn,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events_gzip() {
        // example CloudWatch Logs subscription event
        let record = r#"
{
  "messageType": "DATA_MESSAGE",
  "owner": "071959437513",
  "logGroup": "/jesse/test",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
  "logEvents": [
    {
      "id": "35683658089614582423604394983260738922885519999578275840",
      "timestamp": 1600110569039,
      "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
    },
    {
      "id": "35683658089659183914001456229543810359430816722590236673",
      "timestamp": 1600110569041,
      "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
    }
  ]
}
"#;

        let (rx, addr) = source(None).await;

        let source_arn = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";
        let timestamp: DateTime<Utc> = Utc::now();

        let res = send(
            addr,
            timestamp,
            vec![record],
            None,
            request_id,
            source_arn,
            true,
        )
        .await
        .unwrap();
        assert_eq!(200, res.status().as_u16());

        let events = collect_ready(rx).await.unwrap();
        assert_eq!(
            events,
            vec![log_event! {
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> record,
                "request_id" => request_id,
                "source_arn" => source_arn,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_rejects_bad_access_key() {
        let (_rx, addr) = source(Some("an access key".to_string())).await;

        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some("bad access key"),
            request_id,
            "",
            false,
        )
        .await
        .unwrap();
        assert_eq!(401, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }
}
