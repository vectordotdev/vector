//! Work with HTTP bodies as streams of Kubernetes resources.

use super::{multi_response_decoder::MultiResponseDecoder, response, Response};
use crate::internal_events::kubernetes::stream as internal_events;
use async_stream::try_stream;
use bytes::Buf;
use futures::pin_mut;
use futures::stream::Stream;
use hyper::body::HttpBody as Body;
use snafu::{ResultExt, Snafu};

/// Converts the HTTP response [`Body`] to a stream of parsed Kubernetes
/// [`Response`]s.
pub fn body<B, T>(body: B) -> impl Stream<Item = Result<T, Error<<B as Body>::Error>>>
where
    T: Response + Unpin + 'static,
    B: Body,
    <B as Body>::Error: std::error::Error + 'static + Unpin,
{
    try_stream! {
        let mut decoder: MultiResponseDecoder<T> = MultiResponseDecoder::new();

        debug!(message = "Streaming the HTTP body.");

        pin_mut!(body);
        while let Some(buf) = body.data().await {
            let mut buf = buf.context(Reading)?;
            let chunk = buf.to_bytes();
            let responses = decoder.process_next_chunk(chunk.as_ref());
            emit!(internal_events::ChunkProcessed{ byte_size: chunk.len() });
            for response in responses {
                let response = response.context(Parsing)?;
                yield response;
            }
        }
        decoder.finish().map_err(|data| Error::UnparsedDataUponCompletion { data })?;

        // Temporary. Without it `clippy` show `needless_return` warning.
        // https://github.com/timberio/vector/pull/4818/files/471caf28164db3599611494d68efbcb0ab090ae6#diff-38f65b460ca0ce88f0e24ff9791321090396872183b32b03c8a3435dbe4f45f3
        ()
    }
}

/// Errors that can occur in the stream.
#[derive(Debug, Snafu)]
pub enum Error<ReadError>
where
    ReadError: std::error::Error + 'static,
{
    /// An error occurred while reading the response body.
    #[snafu(display("reading the data chunk failed"))]
    Reading {
        /// The error we got while reading.
        source: ReadError,
    },

    /// An error occurred while parsing the response body.
    #[snafu(display("data parsing failed"))]
    Parsing {
        /// Response parsing error.
        source: response::Error,
    },

    /// An incomplete response remains in the buffer, but we don't expect
    /// any more data.
    #[snafu(display("unparsed data remaining upon completion"))]
    UnparsedDataUponCompletion {
        /// The unparsed data.
        data: Vec<u8>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::trace_init;
    use futures::StreamExt;
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::WatchEvent};

    fn hyper_body_from_chunks(
        chunks: Vec<Result<&'static str, std::io::Error>>,
    ) -> hyper::body::Body {
        let in_stream = futures::stream::iter(chunks);
        hyper::body::Body::wrap_stream(in_stream)
    }

    #[tokio::test]
    async fn test_body() {
        trace_init();

        let data = r#"{
                "type": "ADDED",
                "object": {
                    "kind": "Pod",
                    "apiVersion": "v1",
                    "metadata": {
                        "uid": "uid0"
                    }
                }
            }"#;
        let chunks: Vec<Result<_, std::io::Error>> = vec![Ok(data)];
        let sample_body = hyper_body_from_chunks(chunks);

        let out_stream = body::<_, WatchEvent<Pod>>(sample_body);
        pin_mut!(out_stream);

        assert!(out_stream.next().await.unwrap().is_ok());
        assert!(out_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_body_passes_reading_error() {
        trace_init();

        let err = std::io::Error::new(std::io::ErrorKind::Other, "test error");
        let chunks: Vec<Result<_, std::io::Error>> = vec![Err(err)];
        let sample_body = hyper_body_from_chunks(chunks);

        let out_stream = body::<_, WatchEvent<Pod>>(sample_body);
        pin_mut!(out_stream);

        {
            let err = out_stream.next().await.unwrap().unwrap_err();
            assert!(matches!(err, Error::Reading { source: hyper::Error { .. } }));
        }

        assert!(out_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_body_passes_parsing_error() {
        trace_init();

        let chunks: Vec<Result<_, std::io::Error>> = vec![Ok("qwerty")];
        let sample_body = hyper_body_from_chunks(chunks);

        let out_stream = body::<_, WatchEvent<Pod>>(sample_body);
        pin_mut!(out_stream);

        {
            let err = out_stream.next().await.unwrap().unwrap_err();
            assert!(matches!(err, Error::Parsing { source: response::Error::Json(_) }));
        }

        assert!(out_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_body_uses_finish() {
        trace_init();

        let chunks: Vec<Result<_, std::io::Error>> = vec![Ok("{")];
        let sample_body = hyper_body_from_chunks(chunks);

        let out_stream = body::<_, WatchEvent<Pod>>(sample_body);
        pin_mut!(out_stream);

        {
            let err = out_stream.next().await.unwrap().unwrap_err();
            assert!(matches!(
                err,
                Error::UnparsedDataUponCompletion { data } if data == vec![b'{']
            ));
        }

        assert!(out_stream.next().await.is_none());
    }
}
