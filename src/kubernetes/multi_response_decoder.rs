//! Decode multiple [`Response`]s.

use k8s_openapi::{http::StatusCode, Response, ResponseError};

/// Provides an algorithm to parse multiple [`Response`]s from multiple chunks
/// of data represented as `&[u8]`.
#[derive(Debug)]
pub struct MultiResponseDecoder<T> {
    pending_data: Vec<u8>,
    responses_buffer: Vec<Result<T, ResponseError>>,
}

impl<T> MultiResponseDecoder<T>
where
    T: Response,
{
    /// Create a new [`MultiResponseDecoder`].
    pub fn new() -> Self {
        Self {
            pending_data: Vec::new(),
            responses_buffer: Vec::new(),
        }
    }

    /// Take the next chunk of data and spit out parsed `T`s.
    pub fn process_next_chunk(
        &mut self,
        chunk: &[u8],
    ) -> std::vec::Drain<'_, Result<T, ResponseError>> {
        self.pending_data.extend_from_slice(chunk);
        loop {
            match T::try_from_parts(StatusCode::OK, &self.pending_data) {
                Ok((response, consumed_bytes)) => {
                    debug_assert!(consumed_bytes > 0, "parser must've consumed some data");
                    self.pending_data.drain(..consumed_bytes);
                    self.responses_buffer.push(Ok(response));
                }
                Err(ResponseError::NeedMoreData) => break,
                Err(error) => {
                    self.responses_buffer.push(Err(error));
                    break;
                }
            };
        }
        self.responses_buffer.drain(..)
    }

    /// Complete the parsing.
    ///
    /// Call this when you're not expecting any more data chunks.
    /// Produces an error if there's unparsed data remaining.
    pub fn finish(self) -> Result<(), Vec<u8>> {
        let Self { pending_data, .. } = self;
        if pending_data.is_empty() {
            return Ok(());
        }
        Err(pending_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::{
        api::core::v1::Pod,
        apimachinery::pkg::apis::meta::v1::{ObjectMeta, WatchEvent},
        WatchResponse,
    };

    /// Test object.
    type TO = WatchResponse<Pod>;

    // A helper function to make a test object.
    fn make_to(uid: &str) -> TO {
        WatchResponse::Ok(WatchEvent::Added(Pod {
            metadata: Some(ObjectMeta {
                uid: Some(uid.to_owned()),
                ..ObjectMeta::default()
            }),
            ..Pod::default()
        }))
    }

    fn assert_test_object(
        tested_test_object: Option<Result<TO, ResponseError>>,
        expected_uid: &str,
    ) {
        let actual_to = tested_test_object
            .expect("expected an yielded entry, but none found")
            .expect("parsing failed");
        let expected_to = make_to(expected_uid);
        match (actual_to, expected_to) {
            (WatchResponse::Ok(actual_event), WatchResponse::Ok(expected_event)) => {
                assert_eq!(actual_event, expected_event)
            }
            _ => panic!("expected an event, got something else"),
        }
    }

    #[test]
    fn test_empty() {
        let dec = MultiResponseDecoder::<TO>::new();
        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_incomplete() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(b"{");
            assert!(stream.next().is_none());
        }

        assert_eq!(dec.finish().unwrap_err(), b"{");
    }

    #[test]
    fn test_rubblish() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(b"qwerty");
            assert!(stream.next().unwrap().is_err());
            assert!(stream.next().is_none());
        }

        assert_eq!(dec.finish().unwrap_err(), b"qwerty");
    }

    #[test]
    fn test_one() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_chunked() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "ob"#,
            );
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"ject": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_two() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid1"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert_test_object(stream.next(), "uid1");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_many_chunked_1() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "ob"#,
            );
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"ject": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVe"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"rsion": "v1",
                        "metadata": {
                            "uid": "uid1"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid1");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_many_chunked_2() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }{
                    "type": "ADDED",
                    "ob"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"ject": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid1"
                        }
                    }
                }{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid2"
                        }
                    }
                }{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVe"#,
            );
            assert_test_object(stream.next(), "uid1");
            assert_test_object(stream.next(), "uid2");
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"rsion": "v1",
                        "metadata": {
                            "uid": "uid3"
                        }
                    }
                }{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid4"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid3");
            assert_test_object(stream.next(), "uid4");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_two_one_by_one() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid1"
                        }
                    }
                }"#,
            );
            assert_test_object(stream.next(), "uid1");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_incomplete_after_valid_data() {
        let mut dec = MultiResponseDecoder::<TO>::new();

        {
            let mut stream = dec.process_next_chunk(
                br#"{
                    "type": "ADDED",
                    "object": {
                        "kind": "Pod",
                        "apiVersion": "v1",
                        "metadata": {
                            "uid": "uid0"
                        }
                    }
                }{"#,
            );
            assert_test_object(stream.next(), "uid0");
            assert!(stream.next().is_none());
        }

        assert_eq!(dec.finish().unwrap_err(), b"{");
    }
}
