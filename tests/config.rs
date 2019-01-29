use router::topology::{self, Config};

fn load(config: &str) -> Result<Vec<String>, Vec<String>> {
    Config::load(config.as_bytes())
        .and_then(|c| topology::build(c))
        .map(|(_server, _trigger, _healthcheck, warnings)| warnings)
}

#[test]
fn happy_path() {
    load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["in"]
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "splunk_tcp"
        inputs = ["sampler"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap();

    load(
        r#"
        [sources]
        in = {type = "splunk", address = "127.0.0.1:1235"}

        [transforms]
        sampler = {type = "sampler", inputs = ["in"], rate = 10, pass_list = ["error"]}

        [sinks]
        out = {type = "splunk_tcp", inputs = ["sampler"], address = "127.0.0.1:9999"}
      "#,
    )
    .unwrap();
}

#[test]
fn early_eof() {
    let err = load("[sinks]\n[sin").unwrap_err();

    assert_eq!(err, vec!["expected a right bracket, found eof at line 2"]);
}

#[test]
fn bad_syntax() {
    let err = load(r#"{{{"#).unwrap_err();

    assert_eq!(
        err,
        vec!["expected a table key, found a left brace at line 1"]
    );
}

#[test]
fn missing_key() {
    let err = load(
        r#"
        [sources.in]
        type = "splunk"

        [sinks.out]
        type = "splunk_tcp"
        inputs = ["in"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["missing field `address` for key `sources.in`"]);
}

#[test]
fn bad_type() {
    let err = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1234"

        [sinks.out]
        type = "jabberwocky"
        inputs = ["in"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["unknown variant `jabberwocky`, expected one of `elasticsearch`, `s3`, `splunk_hec`, `splunk_tcp` for key `sinks.out`"]);
}

#[test]
fn nonexistant_input() {
    let err = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["qwerty"]
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "splunk_tcp"
        inputs = ["asdf"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap_err();

    assert_eq!(
        err,
        vec![
            "Input \"asdf\" for sink \"out\" doesn't exist.",
            "Input \"qwerty\" for transform \"sampler\" doesn't exist.",
        ]
    );
}

#[test]
fn bad_regex() {
    let err = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = ["in"]
        rate = 10
        pass_list = ["(["]

        [sinks.out]
        type = "splunk_tcp"
        inputs = ["sampler"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["Transform \"sampler\": regex parse error:\n    ([\n     ^\nerror: unclosed character class"]);

    let err = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [transforms.parser]
        type = "regex_parser"
        inputs = ["in"]
        regex = "(["

        [sinks.out]
        type = "splunk_tcp"
        inputs = ["parser"]
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["Transform \"parser\": regex parse error:\n    ([\n     ^\nerror: unclosed character class"]);
}

#[test]
fn bad_s3_region() {
    let err = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [sinks.out1]
        type = "s3"
        inputs = ["in"]
        buffer_size = 100000
        gzip = true
        bucket = "asdf"
        key_prefix = "logs/"

        [sinks.out2]
        type = "s3"
        inputs = ["in"]
        buffer_size = 100000
        gzip = true
        bucket = "asdf"
        key_prefix = "logs/"
        region = "moonbase-alpha"

        [sinks.out3]
        type = "s3"
        inputs = ["in"]
        buffer_size = 100000
        gzip = true
        bucket = "asdf"
        key_prefix = "logs/"
        region = "us-east-1"
        endpoint = "https://example.com"
      "#,
    )
    .unwrap_err();

    assert_eq!(
        err,
        vec![
            "Sink \"out1\": Must set 'region' or 'endpoint'",
            "Sink \"out2\": Not a valid AWS region: moonbase-alpha",
            "Sink \"out3\": Only one of 'region' or 'endpoint' can be specified",
        ]
    )
}

#[test]
fn warnings() {
    let warnings = load(
        r#"
        [sources.in]
        type = "splunk"
        address = "127.0.0.1:1235"

        [transforms.sampler]
        type = "sampler"
        inputs = []
        rate = 10
        pass_list = ["error"]

        [sinks.out]
        type = "splunk_tcp"
        inputs = []
        address = "127.0.0.1:9999"
      "#,
    )
    .unwrap();

    assert_eq!(
        warnings,
        vec![
            "Sink \"out\" has no inputs",
            "Transform \"sampler\" has no inputs",
            "Transform \"sampler\" has no outputs",
            "Source \"in\" has no outputs",
        ]
    )
}
