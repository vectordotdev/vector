use bytes::{BufMut, BytesMut};
use codec::BytesDelimitedCodec;
use std::collections::HashMap;
use tokio_codec::{Decoder, Encoder};

#[test]
fn bytes_delim_decod() {
    let mut codec = BytesDelimitedCodec::new(b'\n');
    let buf = &mut BytesMut::new();
    buf.put_slice(b"abc\n");
    assert_eq!(Some("abc".into()), codec.decode(buf).unwrap());
}

#[test]
fn bytes_delim_encode() {
    let mut codec = BytesDelimitedCodec::new(b'\n');

    let mut buf = BytesMut::new();
    codec.encode("abc".into(), &mut buf).unwrap();

    assert_eq!(b"abc\n", &buf[..]);
}

#[test]
fn bytes_decode_max_length() {
    const MAX_LENGTH: usize = 6;

    let mut codec = BytesDelimitedCodec::new_with_max_length(b'\n', MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    // limit is 6 so this should fail
    buf.put_slice(b"1234567\n123456\n123412314\n123");

    assert!(codec.decode(buf).unwrap().is_none());
    assert!(codec.decode(buf).unwrap().is_some());
    assert!(codec.decode_eof(buf).unwrap().is_none());
    assert!(codec.decode_eof(buf).unwrap().is_some());
}

// Regression test for [infinite loop bug](https://github.com/timberio/vector/issues/2564)
// Derived from https://github.com/tokio-rs/tokio/issues/1483
#[test]
fn bytes_decoder_discard_repeat() {
    const MAX_LENGTH: usize = 1;

    let mut codec = BytesDelimitedCodec::new_with_max_length(b'\n', MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put("aa");
    assert!(codec.decode(buf).unwrap().is_none());
    buf.put("a");
    assert!(codec.decode(buf).unwrap().is_none());
}

#[test]
fn bytes_decode_json_escaped() {
    let mut input = HashMap::new();
    input.insert("key", "value");
    input.insert("new", "li\nne");

    let mut bytes = serde_json::to_vec(&input).unwrap();
    bytes.push(b'\n');

    let mut codec = BytesDelimitedCodec::new(b'\n');
    let buf = &mut BytesMut::new();

    buf.reserve(bytes.len());
    buf.extend(bytes);

    let result = codec.decode(buf).unwrap();

    assert!(result.is_some());
    assert!(buf.is_empty());
}

#[test]
fn bytes_decode_json_multiline() {
    let events = r#"
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
"#;

    let mut codec = BytesDelimitedCodec::new(b'\n');
    let buf = &mut BytesMut::new();

    buf.extend(events.to_string().as_bytes());

    let mut i = 0;
    while codec.decode(buf).unwrap().is_some() {
        i += 1;
    }

    assert_eq!(i, 52);
}
