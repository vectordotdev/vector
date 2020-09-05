use vector::{
    config::{self, ConfigDiff},
    topology,
};

async fn load(config: &str) -> Result<Vec<String>, Vec<String>> {
    match config::load_from_str(config) {
        Ok(c) => {
            let diff = ConfigDiff::initial(&c);
            match (
                config::warnings(&c),
                topology::builder::build_pieces(&c, &diff).await,
            ) {
                (Some(warnings), Ok(_pieces)) => Ok(warnings),
                (None, Ok(_pieces)) => Ok(vec![]),
                (_, Err(errors)) => Err(errors),
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn happy_path() {
    load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["in"]
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["sampler"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap();

    load(
        r#"
        [sources]
        in = {type = "socket", mode = "tcp", address = "127.0.0.1:1235"}

        [transforms]
        sampler = {type = "sampler", inputs = ["in"], rate = 10, pass_list = ["error"]}

        [sinks]
        out = {type = "socket", mode = "tcp", inputs = ["sampler"], encoding = "text", address = "127.0.0.1:9999"}
      "#,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn early_eof() {
    let err = load("[sinks]\n[sin").await.unwrap_err();

    assert_eq!(err, vec!["expected a right bracket, found eof at line 2"]);
}

#[tokio::test]
async fn bad_syntax() {
    let err = load(r#"{{{"#).await.unwrap_err();

    assert_eq!(
        err,
        vec!["expected a table key, found a left brace at line 1"]
    );
}

#[cfg(all(feature = "sources-socket", feature = "sinks-socket"))]
#[tokio::test]
async fn missing_key() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"

        [sinks.out]
        type = "socket"
        inputs = ["in"]
        mode = "tcp"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(err, vec!["missing field `mode` for key `sources.in`"]);
}

#[cfg(all(feature = "sources-socket", feature = "sinks-socket"))]
#[tokio::test]
async fn missing_key2() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"

        [sinks.out]
        type = "socket"
        mode = "out"
        inputs = ["in"]
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(err, vec!["missing field `address` for key `sources.in`"]);
}

#[cfg(feature = "sources-socket")]
#[tokio::test]
async fn bad_type() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1234"

        [sinks.out]
        type = "jabberwocky"
        inputs = ["in"]
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(err.len(), 1);
    assert!(
        err[0].starts_with("unknown variant `jabberwocky`, expected "),
        "Found: {:?}",
        &err[0]
    );
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn nonexistant_input() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["qwerty"]
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["asdf"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(
        err,
        vec![
            "Input \"asdf\" for sink \"out\" doesn't exist.",
            "Input \"qwerty\" for transform \"sampler\" doesn't exist.",
        ]
    );
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn bad_regex() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["in"]
        rate = 10
        pass_list = ["(["]

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["sampler"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(err.len(), 1);
    assert!(err[0].contains("error: unclosed character class"));

    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.parser]
        type = "regex_parser"
        inputs = ["in"]
        patterns = ["(["]

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["parser"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(err.len(), 1);
    assert!(err[0].contains("error: unclosed character class"));
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-regex_parser",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn good_regex_parser() {
    let result = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.parser]
        type = "regex_parser"
        inputs = ["in"]
        regex = "(?P<out>.+)"

        [transforms.parser.types]
        out = "integer"

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["parser"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await;

    assert!(result.is_ok());
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-tokenizer",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn good_tokenizer() {
    let result = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.parser]
        type = "tokenizer"
        inputs = ["in"]
        field_names = ["one", "two", "three", "four"]

        [transforms.parser.types]
        one = "integer"
        two = "boolean"

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["parser"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await;

    assert!(result.is_ok());
}
#[cfg(all(feature = "sources-socket", feature = "sinks-aws_s3"))]
#[tokio::test]
async fn bad_s3_region() {
    let err = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [sinks.out1]
        type = "aws_s3"
        inputs = ["in"]
        compression = "gzip"
        encoding = "text"
        bucket = "asdf"
        key_prefix = "logs/"

        [sinks.out2]
        type = "aws_s3"
        inputs = ["in"]
        compression = "gzip"
        encoding = "text"
        bucket = "asdf"
        key_prefix = "logs/"
        region = "moonbase-alpha"

        [sinks.out3]
        type = "aws_s3"
        inputs = ["in"]
        compression = "gzip"
        encoding = "text"
        bucket = "asdf"
        key_prefix = "logs/"
        region = "us-east-1"
        endpoint = "https://localhost"

        [sinks.out4]
        type = "aws_s3"
        inputs = ["in"]
        compression = "gzip"
        encoding = "text"
        bucket = "asdf"
        key_prefix = "logs/"
        endpoint = "this shoudlnt work"

        [sinks.out4.batch]
        max_size = 100000
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(
        err,
        vec![
            "Sink \"out1\": Must set either 'region' or 'endpoint'",
            "Sink \"out2\": Not a valid AWS region: moonbase-alpha",
            "Sink \"out3\": Only one of 'region' or 'endpoint' can be specified",
            "Sink \"out4\": Failed to parse custom endpoint as URI: invalid uri character"
        ]
    )
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn warnings() {
    let warnings = load(
        r#"
        [sources.in1]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [sources.in2]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1236"

        [transforms.sampler1]
        type = "sampler"
        inputs = ["in1"]
        rate = 10
        pass_list = ["error"]

        [transforms.sampler2]
        type = "sampler"
        inputs = ["in1"]
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["sampler1"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap();

    assert_eq!(
        warnings,
        vec![
            "Transform \"sampler2\" has no consumers",
            "Source \"in2\" has no consumers",
        ]
    )
}

#[cfg(all(
    feature = "sources-socket",
    feature = "transforms-sampler",
    feature = "sinks-socket"
))]
#[tokio::test]
async fn cycle() {
    let errors = load(
        r#"
        [sources.in]
        type = "socket"
        mode = "tcp"
        address = "127.0.0.1:1235"

        [transforms.one]
        type = "sampler"
        inputs = ["in"]
        rate = 10
        pass_list = []

        [transforms.two]
        type = "sampler"
        inputs = ["one", "four"]
        rate = 10
        pass_list = []

        [transforms.three]
        type = "sampler"
        inputs = ["two"]
        rate = 10
        pass_list = []

        [transforms.four]
        type = "sampler"
        inputs = ["three"]
        rate = 10
        pass_list = []

        [sinks.out]
        type = "socket"
        mode = "tcp"
        inputs = ["four"]
        encoding = "text"
        address = "127.0.0.1:9999"
      "#,
    )
    .await
    .unwrap_err();

    assert_eq!(
        errors,
        vec!["Cyclic dependency detected in the chain [ four -> two -> three -> four ]"]
    )
}

#[cfg(all(feature = "sources-socket", feature = "sinks-socket"))]
#[tokio::test]
async fn disabled_healthcheck() {
    load(
        r#"
      [sources.in]
      type = "socket"
      mode = "tcp"
      address = "127.0.0.1:1234"

      [sinks.out]
      type = "socket"
      mode = "tcp"
      inputs = ["in"]
      address = "0.0.0.0:0"
      encoding = "text"
      healthcheck = false
      "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-http"))]
#[tokio::test]
async fn parses_sink_no_request() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "http"
        inputs = ["in"]
        uri = "https://localhost"
        encoding = "json"
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-http"))]
#[tokio::test]
async fn parses_sink_partial_request() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "http"
        inputs = ["in"]
        uri = "https://localhost"
        encoding = "json"

        [sinks.out.request]
        in_flight_limit = 42
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-http"))]
#[tokio::test]
async fn parses_sink_full_request() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "http"
        inputs = ["in"]
        uri = "https://localhost"
        encoding = "json"

        [sinks.out.request]
        in_flight_limit = 42
        timeout_secs = 2
        rate_limit_duration_secs = 3
        rate_limit_num = 4
        retry_attempts = 5
        retry_max_duration_secs = 10
        retry_initial_backoff_secs = 6
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-http"))]
#[tokio::test]
async fn parses_sink_full_batch_bytes() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "http"
        inputs = ["in"]
        uri = "https://localhost"
        encoding = "json"

        [sinks.out.batch]
        max_size = 100
        timeout_secs = 10
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-aws_cloudwatch_logs"))]
#[tokio::test]
async fn parses_sink_full_batch_event() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "aws_cloudwatch_logs"
        inputs = ["in"]
        region = "us-east-1"
        group_name = "test"
        stream_name = "test"
        encoding = "json"

        [sinks.out.batch]
        max_events = 100
        timeout_secs = 10
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-http"))]
#[tokio::test]
async fn parses_sink_full_auth() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "http"
        inputs = ["in"]
        uri = "https://localhost"
        encoding = "json"

        [sinks.out.auth]
        strategy = "basic"
        user = "user"
        password = "password"
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(feature = "sources-stdin", feature = "sinks-elasticsearch"))]
#[tokio::test]
async fn parses_sink_full_es_basic_auth() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "elasticsearch"
        inputs = ["in"]
        host = "https://localhost"

        [sinks.out.auth]
        strategy = "basic"
        user = "user"
        password = "password"
        "#,
    )
    .await
    .unwrap();
}

#[cfg(all(
    feature = "docker",
    feature = "sources-stdin",
    feature = "sinks-elasticsearch"
))]
#[tokio::test]
async fn parses_sink_full_es_aws() {
    load(
        r#"
        [sources.in]
        type = "stdin"

        [sinks.out]
        type = "elasticsearch"
        inputs = ["in"]
        host = "https://es.us-east-1.amazonaws.com"

        [sinks.out.auth]
        strategy = "aws"
        "#,
    )
    .await
    .unwrap();
}
