//! Decode multiple [`Response`]s.

use super::{response, Response};

/// Provides an algorithm to parse multiple [`Response`]s from multiple chunks
/// of data represented as `&[u8]`.
#[derive(Debug, Default)]
pub struct MultiResponseDecoder<T> {
    pending_data: Vec<u8>,
    responses_buffer: Vec<Result<T, response::Error>>,
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
    ) -> std::vec::Drain<'_, Result<T, response::Error>> {
        self.pending_data.extend_from_slice(chunk);
        loop {
            match T::from_buf(&self.pending_data) {
                Ok((response, consumed_bytes)) => {
                    debug_assert!(consumed_bytes > 0, "Parser must've consumed some data.");
                    self.pending_data.drain(..consumed_bytes);
                    self.responses_buffer.push(Ok(response));
                }
                Err(response::Error::NeedMoreData) => break,
                Err(error) => {
                    error!(message = "Error while decoding response.", pending_data = ?self.pending_data, %error);
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
        // Kubernetes sometimes adds `\n` to the response, consider this
        // a valid termination case.
        if pending_data.is_empty() || pending_data == b"\n" {
            return Ok(());
        }
        Err(pending_data)
    }
}

#[cfg(test)]
mod tests {
    use k8s_openapi::{
        api::core::v1::Pod,
        apimachinery::pkg::apis::meta::v1::{ObjectMeta, WatchEvent},
    };

    use super::*;

    /// Test object.
    type TestObject = WatchEvent<Pod>;

    // A helper function to make a test object.
    fn make_to(uid: &str) -> TestObject {
        WatchEvent::Added(Pod {
            metadata: ObjectMeta {
                uid: Some(uid.to_owned()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        })
    }

    fn assert_test_object(
        tested_test_object: Option<Result<TestObject, response::Error>>,
        expected_uid: &str,
    ) {
        let actual_to = tested_test_object
            .expect("expected an yielded entry, but none found")
            .expect("parsing failed");
        let expected_to = make_to(expected_uid);
        assert_eq!(actual_to, expected_to);
    }

    #[test]
    fn test_empty() {
        let dec = MultiResponseDecoder::<TestObject>::new();
        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_incomplete() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

        {
            let mut stream = dec.process_next_chunk(b"{");
            assert!(stream.next().is_none());
        }

        assert_eq!(dec.finish().unwrap_err(), b"{");
    }

    #[test]
    fn test_rubbish() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

        {
            let mut stream = dec.process_next_chunk(b"qwerty");
            assert!(stream.next().unwrap().is_err());
            assert!(stream.next().is_none());
        }

        assert_eq!(dec.finish().unwrap_err(), b"qwerty");
    }

    #[test]
    fn test_one() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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
        let mut dec = MultiResponseDecoder::<TestObject>::new();

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

    #[test]
    fn test_allows_unparsed_newlines_at_finish() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

        {
            let mut stream = dec.process_next_chunk(b"\n");
            assert!(stream.next().is_none());
        }

        assert!(dec.finish().is_ok());
    }

    #[test]
    fn test_memory_usage() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

        let chunk = br#"{
            "type": "ADDED",
            "object": {
                "kind": "Pod",
                "apiVersion": "v1",
                "metadata": {
                    "uid": "uid0"
                }
            }
        }"#;
        let mut chunks = chunk.iter().cycle();

        let max_chunks_per_iter = 15;

        // Simulate processing a huge number of items.
        for _ in 0..100_000 {
            // Take random amount of bytes from the chunks iter and prepare the
            // next chunk.
            let to_take = rand::random::<usize>() % (chunk.len() * max_chunks_per_iter);
            let next_chunk = (&mut chunks).take(to_take).cloned().collect::<Box<_>>();

            // Process the chunk data.
            let stream = dec.process_next_chunk(next_chunk.as_ref());
            drop(stream); // consume all the emitted items
        }

        // Check that `pending_data` capacity didn't grow out way of hand.
        // If we had issues with memory management, it would be the one
        // to blow first.
        assert!(dec.pending_data.capacity() <= chunk.len() * 100);

        // Ensure that response buffer never grows beyond it's capacity limit.
        // Capacity limit is set based on heuristics about `Vec` internals, and
        // is adjusted to be as low as possible.
        assert!(dec.responses_buffer.capacity() <= (max_chunks_per_iter + 2).next_power_of_two());
    }

    #[test]
    fn test_practical_error_case_1() {
        let mut dec = MultiResponseDecoder::<TestObject>::new();

        {
            let mut stream = dec.process_next_chunk(&[
                123, 34, 116, 121, 112, 101, 34, 58, 34, 66, 79, 79, 75, 77, 65, 82, 75, 34, 44,
                34, 111, 98, 106, 101, 99, 116, 34, 58, 123, 34, 107, 105, 110, 100, 34, 58, 34,
                80, 111, 100, 34, 44, 34, 97, 112, 105, 86, 101, 114, 115, 105, 111, 110, 34, 58,
                34, 118, 49, 34, 44, 34, 109, 101, 116, 97, 100, 97, 116, 97, 34, 58, 123, 34, 114,
                101, 115, 111, 117, 114, 99, 101, 86, 101, 114, 115, 105, 111, 110, 34, 58, 34, 51,
                56, 52, 53, 34, 44, 34, 99, 114, 101, 97, 116, 105, 111, 110, 84, 105, 109, 101,
                115, 116, 97, 109, 112, 34, 58, 110, 117, 108, 108, 125, 44, 34, 115, 112, 101, 99,
                34, 58, 123, 34, 99, 111, 110, 116, 97, 105, 110, 101, 114, 115, 34, 58, 110, 117,
                108, 108, 125, 44, 34, 115, 116, 97, 116, 117, 115, 34, 58, 123, 125, 125, 125, 10,
            ]);
            let actual_to = stream
                .next()
                .expect("expected an yielded entry, but none found")
                .expect("parsing failed");
            let expected_to = WatchEvent::Bookmark {
                resource_version: "3845".into(),
            };
            assert_eq!(actual_to, expected_to);
        }

        assert!(dec.finish().is_ok());
    }
}
