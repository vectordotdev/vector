use bytes::{Buf, BufMut, Bytes, BytesMut};
use memchr::memchr;
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Decoder, Encoder};

use crate::codecs::{decoding, encoding};

/// Config used to build a `CharacterDelimitedDecoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharacterDelimitedDecoderConfig {
    character_delimited: CharacterDelimitedDecoderOptions,
}

/// Options for building a `CharacterDelimitedDecoder`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CharacterDelimitedDecoderOptions {
    /// The character that delimits byte sequences.
    #[serde(with = "crate::serde::ascii_char")]
    delimiter: u8,
    /// The maximum length of the byte buffer.
    ///
    /// This length does *not* include the trailing delimiter.
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    max_length: Option<usize>,
}

#[typetag::serde(name = "character_delimited")]
impl decoding::FramingConfig for CharacterDelimitedDecoderConfig {
    fn build(&self) -> crate::Result<decoding::BoxedFramer> {
        if let Some(max_length) = self.character_delimited.max_length {
            Ok(Box::new(CharacterDelimitedDecoder::new_with_max_length(
                self.character_delimited.delimiter,
                max_length,
            )))
        } else {
            Ok(Box::new(CharacterDelimitedDecoder::new(
                self.character_delimited.delimiter,
            )))
        }
    }
}

/// A decoder for handling bytes that are delimited by (a) chosen character(s).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CharacterDelimitedDecoder {
    /// The delimiter used to separate byte sequences.
    delimiter: u8,
    /// The maximum length of the byte buffer.
    max_length: usize,
}

impl CharacterDelimitedDecoder {
    /// Creates a `CharacterDelimitedDecoder` with the specified delimiter.
    pub const fn new(delimiter: u8) -> Self {
        CharacterDelimitedDecoder {
            delimiter,
            max_length: usize::MAX,
        }
    }

    /// Creates a `CharacterDelimitedDecoder` with a maximum frame length limit.
    ///
    /// Any frames longer than `max_length` bytes will be discarded entirely.
    pub const fn new_with_max_length(delimiter: u8, max_length: usize) -> Self {
        CharacterDelimitedDecoder {
            max_length,
            ..CharacterDelimitedDecoder::new(delimiter)
        }
    }

    /// Returns the maximum frame length when decoding.
    pub const fn max_length(&self) -> usize {
        self.max_length
    }
}

impl Decoder for CharacterDelimitedDecoder {
    type Item = Bytes;
    type Error = decoding::BoxedFramingError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, Self::Error> {
        loop {
            // This function has the following goal: we are searching for
            // sub-buffers delimited by `self.delimiter` with size no more than
            // `self.max_length`. If a sub-buffer is found that exceeds
            // `self.max_length` we discard it, else we return it. At the end of
            // the buffer if the delimiter is not present the remainder of the
            // buffer is discarded.
            match memchr(self.delimiter, buf) {
                None => return Ok(None),
                Some(next_delimiter_idx) => {
                    if next_delimiter_idx > self.max_length {
                        // The discovered sub-buffer is too big, so we discard
                        // it, taking care to also discard the delimiter.
                        warn!(
                            message = "Discarding frame larger than max_length.",
                            buf_len = buf.len(),
                            max_length = self.max_length,
                            internal_log_rate_secs = 30
                        );
                        buf.advance(next_delimiter_idx + 1);
                    } else {
                        let frame = buf.split_to(next_delimiter_idx).freeze();
                        trace!(
                            message = "Decoding the frame.",
                            bytes_proccesed = frame.len()
                        );
                        buf.advance(1); // scoot past the delimiter
                        return Ok(Some(frame));
                    }
                }
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, Self::Error> {
        match self.decode(buf)? {
            Some(frame) => Ok(Some(frame)),
            None => {
                if buf.is_empty() {
                    Ok(None)
                } else if buf.len() > self.max_length {
                    warn!(
                        message = "Discarding frame larger than max_length.",
                        buf_len = buf.len(),
                        max_length = self.max_length,
                        internal_log_rate_secs = 30
                    );
                    Ok(None)
                } else {
                    let bytes: Bytes = buf.split_to(buf.len()).freeze();
                    Ok(Some(bytes))
                }
            }
        }
    }
}

/// Config used to build a `CharacterDelimitedEncoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharacterDelimitedEncoderConfig {
    character_delimited: CharacterDelimitedEncoderOptions,
}

/// Options for building a `CharacterDelimitedEncoder`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CharacterDelimitedEncoderOptions {
    /// The character that delimits byte sequences.
    delimiter: u8,
}

#[typetag::serde(name = "character_delimited")]
impl encoding::FramingConfig for CharacterDelimitedEncoderConfig {
    fn build(&self) -> crate::Result<encoding::BoxedFramer> {
        Ok(Box::new(CharacterDelimitedEncoder::new(
            self.character_delimited.delimiter,
        )))
    }
}

/// An encoder for handling bytes that are delimited by (a) chosen character(s).
#[derive(Debug, Clone)]
pub struct CharacterDelimitedEncoder {
    delimiter: u8,
}

impl CharacterDelimitedEncoder {
    /// Creates a `CharacterDelimitedEncoder` with the specified delimiter.
    pub const fn new(delimiter: u8) -> Self {
        Self { delimiter }
    }
}

impl Encoder<()> for CharacterDelimitedEncoder {
    type Error = encoding::BoxedFramingError;

    fn encode(&mut self, _: (), buf: &mut BytesMut) -> Result<(), encoding::BoxedFramingError> {
        buf.put_u8(self.delimiter);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use indoc::indoc;

    use super::*;

    #[test]
    fn character_delimited_decode() {
        let mut codec = CharacterDelimitedDecoder::new(b'\n');
        let buf = &mut BytesMut::new();
        buf.put_slice(b"abc\n");
        assert_eq!(Some("abc".into()), codec.decode(buf).unwrap());
    }

    #[test]
    fn character_delimited_encode() {
        let mut codec = CharacterDelimitedEncoder::new(b'\n');

        let mut buf = BytesMut::from("abc");
        codec.encode((), &mut buf).unwrap();

        assert_eq!(b"abc\n", &buf[..]);
    }

    #[test]
    fn decode_max_length() {
        const MAX_LENGTH: usize = 6;

        let mut codec = CharacterDelimitedDecoder::new_with_max_length(b'\n', MAX_LENGTH);
        let buf = &mut BytesMut::new();

        // limit is 6 so it will skip longer lines
        buf.put_slice(b"1234567\n123456\n123412314\n123");

        assert_eq!(codec.decode(buf).unwrap(), Some(Bytes::from("123456")));
        assert_eq!(codec.decode(buf).unwrap(), None);

        let buf = &mut BytesMut::new();

        // limit is 6 so it will skip longer lines
        buf.put_slice(b"1234567\n123456\n123412314\n123");

        assert_eq!(codec.decode_eof(buf).unwrap(), Some(Bytes::from("123456")));
        assert_eq!(codec.decode_eof(buf).unwrap(), Some(Bytes::from("123")));
        assert_eq!(codec.decode_eof(buf).unwrap(), None);
    }

    // Regression test for [infinite loop bug](https://github.com/timberio/vector/issues/2564)
    // Derived from https://github.com/tokio-rs/tokio/issues/1483
    #[test]
    fn decoder_discard_repeat() {
        const MAX_LENGTH: usize = 1;

        let mut codec = CharacterDelimitedDecoder::new_with_max_length(b'\n', MAX_LENGTH);
        let buf = &mut BytesMut::new();

        buf.reserve(200);
        buf.put(&b"aa"[..]);
        assert!(codec.decode(buf).unwrap().is_none());
        buf.put(&b"a"[..]);
        assert!(codec.decode(buf).unwrap().is_none());
    }

    #[test]
    fn decode_json_escaped() {
        let mut input = HashMap::new();
        input.insert("key", "value");
        input.insert("new", "li\nne");

        let mut bytes = serde_json::to_vec(&input).unwrap();
        bytes.push(b'\n');

        let mut codec = CharacterDelimitedDecoder::new(b'\n');
        let buf = &mut BytesMut::new();

        buf.reserve(bytes.len());
        buf.extend(bytes);

        let result = codec.decode(buf).unwrap();

        assert!(result.is_some());
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_json_multiline() {
        let events = indoc! {r#"
            {"log":"\u0009at org.springframework.security.web.context.SecurityContextPersistenceFilter.doFilter(SecurityContextPersistenceFilter.java:105)\n","stream":"stdout","time":"2019-01-18T07:49:27.374616758Z"}
            {"log":"\u0009at org.springframework.security.web.FilterChainProxy$VirtualFilterChain.doFilter(FilterChainProxy.java:334)\n","stream":"stdout","time":"2019-01-18T07:49:27.374640288Z"}
            {"log":"\u0009at org.springframework.security.web.context.request.async.WebAsyncManagerIntegrationFilter.doFilterInternal(WebAsyncManagerIntegrationFilter.java:56)\n","stream":"stdout","time":"2019-01-18T07:49:27.374655505Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.374671955Z"}
            {"log":"\u0009at org.springframework.security.web.FilterChainProxy$VirtualFilterChain.doFilter(FilterChainProxy.java:334)\n","stream":"stdout","time":"2019-01-18T07:49:27.374690312Z"}
            {"log":"\u0009at org.springframework.security.web.FilterChainProxy.doFilterInternal(FilterChainProxy.java:215)\n","stream":"stdout","time":"2019-01-18T07:49:27.374704522Z"}
            {"log":"\u0009at org.springframework.security.web.FilterChainProxy.doFilter(FilterChainProxy.java:178)\n","stream":"stdout","time":"2019-01-18T07:49:27.374718459Z"}
            {"log":"\u0009at org.springframework.web.filter.DelegatingFilterProxy.invokeDelegate(DelegatingFilterProxy.java:357)\n","stream":"stdout","time":"2019-01-18T07:49:27.374732919Z"}
            {"log":"\u0009at org.springframework.web.filter.DelegatingFilterProxy.doFilter(DelegatingFilterProxy.java:270)\n","stream":"stdout","time":"2019-01-18T07:49:27.374750799Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.374764819Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.374778682Z"}
            {"log":"\u0009at org.springframework.web.filter.RequestContextFilter.doFilterInternal(RequestContextFilter.java:99)\n","stream":"stdout","time":"2019-01-18T07:49:27.374792429Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.374805985Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.374819625Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.374833335Z"}
            {"log":"\u0009at org.springframework.web.filter.HttpPutFormContentFilter.doFilterInternal(HttpPutFormContentFilter.java:109)\n","stream":"stdout","time":"2019-01-18T07:49:27.374847845Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.374861925Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.37487589Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.374890043Z"}
            {"log":"\u0009at org.springframework.web.filter.HiddenHttpMethodFilter.doFilterInternal(HiddenHttpMethodFilter.java:93)\n","stream":"stdout","time":"2019-01-18T07:49:27.374903813Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.374917793Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.374931586Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.374946006Z"}
            {"log":"\u0009at org.springframework.boot.actuate.metrics.web.servlet.WebMvcMetricsFilter.filterAndRecordMetrics(WebMvcMetricsFilter.java:117)\n","stream":"stdout","time":"2019-01-18T07:49:27.37496104Z"}
            {"log":"\u0009at org.springframework.boot.actuate.metrics.web.servlet.WebMvcMetricsFilter.doFilterInternal(WebMvcMetricsFilter.java:106)\n","stream":"stdout","time":"2019-01-18T07:49:27.37498773Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.375003113Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.375017063Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.37503086Z"}
            {"log":"\u0009at org.springframework.web.filter.CharacterEncodingFilter.doFilterInternal(CharacterEncodingFilter.java:200)\n","stream":"stdout","time":"2019-01-18T07:49:27.3750454Z"}
            {"log":"\u0009at org.springframework.web.filter.OncePerRequestFilter.doFilter(OncePerRequestFilter.java:107)\n","stream":"stdout","time":"2019-01-18T07:49:27.37505928Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.internalDoFilter(ApplicationFilterChain.java:193)\n","stream":"stdout","time":"2019-01-18T07:49:27.37507306Z"}
            {"log":"\u0009at org.apache.catalina.core.ApplicationFilterChain.doFilter(ApplicationFilterChain.java:166)\n","stream":"stdout","time":"2019-01-18T07:49:27.375086726Z"}
            {"log":"\u0009at org.apache.catalina.core.StandardWrapperValve.invoke(StandardWrapperValve.java:198)\n","stream":"stdout","time":"2019-01-18T07:49:27.375100817Z"}
            {"log":"\u0009at org.apache.catalina.core.StandardContextValve.invoke(StandardContextValve.java:96)\n","stream":"stdout","time":"2019-01-18T07:49:27.375115354Z"}
            {"log":"\u0009at org.apache.catalina.authenticator.AuthenticatorBase.invoke(AuthenticatorBase.java:493)\n","stream":"stdout","time":"2019-01-18T07:49:27.375129454Z"}
            {"log":"\u0009at org.apache.catalina.core.StandardHostValve.invoke(StandardHostValve.java:140)\n","stream":"stdout","time":"2019-01-18T07:49:27.375144001Z"}
            {"log":"\u0009at org.apache.catalina.valves.ErrorReportValve.invoke(ErrorReportValve.java:81)\n","stream":"stdout","time":"2019-01-18T07:49:27.375157464Z"}
            {"log":"\u0009at org.apache.catalina.core.StandardEngineValve.invoke(StandardEngineValve.java:87)\n","stream":"stdout","time":"2019-01-18T07:49:27.375170981Z"}
            {"log":"\u0009at org.apache.catalina.connector.CoyoteAdapter.service(CoyoteAdapter.java:342)\n","stream":"stdout","time":"2019-01-18T07:49:27.375184417Z"}
            {"log":"\u0009at org.apache.coyote.http11.Http11Processor.service(Http11Processor.java:800)\n","stream":"stdout","time":"2019-01-18T07:49:27.375198024Z"}
            {"log":"\u0009at org.apache.coyote.AbstractProcessorLight.process(AbstractProcessorLight.java:66)\n","stream":"stdout","time":"2019-01-18T07:49:27.375211594Z"}
            {"log":"\u0009at org.apache.coyote.AbstractProtocol$ConnectionHandler.process(AbstractProtocol.java:806)\n","stream":"stdout","time":"2019-01-18T07:49:27.375225237Z"}
            {"log":"\u0009at org.apache.tomcat.util.net.NioEndpoint$SocketProcessor.doRun(NioEndpoint.java:1498)\n","stream":"stdout","time":"2019-01-18T07:49:27.375239487Z"}
            {"log":"\u0009at org.apache.tomcat.util.net.SocketProcessorBase.run(SocketProcessorBase.java:49)\n","stream":"stdout","time":"2019-01-18T07:49:27.375253464Z"}
            {"log":"\u0009at java.util.concurrent.ThreadPoolExecutor.runWorker(ThreadPoolExecutor.java:1149)\n","stream":"stdout","time":"2019-01-18T07:49:27.375323255Z"}
            {"log":"\u0009at java.util.concurrent.ThreadPoolExecutor$Worker.run(ThreadPoolExecutor.java:624)\n","stream":"stdout","time":"2019-01-18T07:49:27.375345642Z"}
            {"log":"\u0009at org.apache.tomcat.util.threads.TaskThread$WrappingRunnable.run(TaskThread.java:61)\n","stream":"stdout","time":"2019-01-18T07:49:27.375363208Z"}
            {"log":"\u0009at java.lang.Thread.run(Thread.java:748)\n","stream":"stdout","time":"2019-01-18T07:49:27.375377695Z"}
            {"log":"\n","stream":"stdout","time":"2019-01-18T07:49:27.375391335Z"}
            {"log":"\n","stream":"stdout","time":"2019-01-18T07:49:27.375416915Z"}
            {"log":"2019-01-18 07:53:06.419 [               ]  INFO 1 --- [vent-bus.prod-1] c.t.listener.CommonListener              : warehousing Dailywarehousing.daily\n","stream":"stdout","time":"2019-01-18T07:53:06.420527437Z"}
        "#};

        let mut codec = CharacterDelimitedDecoder::new(b'\n');
        let buf = &mut BytesMut::new();

        buf.extend(events.to_string().as_bytes());

        let mut i = 0;
        while codec.decode(buf).unwrap().is_some() {
            i += 1;
        }

        assert_eq!(i, 51);
    }
}
