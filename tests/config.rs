use router::topology::{self, Config};

fn load(config: &str) -> Result<Vec<String>, Vec<String>> {
    Config::load(config.as_bytes())
        .and_then(|c| topology::build(c))
        .map(|(_server, _trigger, _healthcheck, warnings)| warnings)
}

#[test]
fn happy_path() {
    let topology = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "transforms": {
            "sampler": {
              "type": "sampler",
              "inputs": ["in"],
              "rate": 10,
              "pass_list": ["error"]
            }
          },
          "sinks": {
            "out": {
              "type": "splunk_tcp",
              "inputs": ["sampler"],
              "address": "127.0.0.1:9999"
            }
          }
        }
      "#,
    );

    assert!(topology.is_ok());
}

#[test]
fn early_eof() {
    let err = load(r#"{"asdf": "#).unwrap_err();

    assert_eq!(err, vec!["EOF while parsing a value at line 1 column 9"]);
}

#[test]
fn bad_syntax() {
    let err = load(r#"{{{"#).unwrap_err();

    assert_eq!(err.len(), 1);
    assert_eq!(err[0], "key must be a string at line 1 column 2");

    let err = load(r#"{"trailing": "comma",}"#).unwrap_err();

    assert_eq!(err, vec!["trailing comma at line 1 column 22"]);
}

#[test]
fn missing_key() {
    let err = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk"
            }
          },
          "sinks": {
            "out": {
              "type": "elasticsearch",
              "inputs": ["in"]
            }
          }
        }
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["missing field `address` at line 6 column 13"]);
}

#[test]
fn bad_type() {
    let err = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "sinks": {
            "out": {
              "type": "jabberwocky",
              "inputs": ["in"]
            }
          }
        }
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["unknown variant `jabberwocky`, expected one of `elasticsearch`, `s3`, `splunk_hec`, `splunk_tcp` at line 13 column 13"]);
}

#[test]
fn nonexistant_input() {
    let err = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "transforms": {
            "sampler": {
              "type": "sampler",
              "inputs": ["qwerty"],
              "rate": 10,
              "pass_list": []
            }
          },
          "sinks": {
            "out": {
              "type": "elasticsearch",
              "inputs": ["asdf"]
            }
          }
        }
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
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "transforms": {
            "sampler": {
              "type": "sampler",
              "inputs": ["in"],
              "rate": 10,
              "pass_list": ["(["]
            }
          },
          "sinks": {
            "out": {
              "type": "elasticsearch",
              "inputs": ["sampler"]
            }
          }
        }
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["Transform \"sampler\": regex parse error:\n    ([\n     ^\nerror: unclosed character class"]);

    let err = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "transforms": {
            "parser": {
              "type": "regex_parser",
              "inputs": ["in"],
              "regex": "(["
            }
          },
          "sinks": {
            "out": {
              "type": "elasticsearch",
              "inputs": ["parser"]
            }
          }
        }
      "#,
    )
    .unwrap_err();

    assert_eq!(err, vec!["Transform \"parser\": regex parse error:\n    ([\n     ^\nerror: unclosed character class"]);
}

#[test]
fn bad_s3_region() {
    let err = load(
        r#"
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "sinks": {
            "out1": {
              "type": "s3",
              "inputs": ["in"],
              "buffer_size": 100000,
              "gzip": true,
              "bucket": "asdf",
              "key_prefix": "logs/"
            },
            "out2": {
              "type": "s3",
              "inputs": ["in"],
              "buffer_size": 100000,
              "gzip": true,
              "bucket": "asdf",
              "key_prefix": "logs/",
              "region": "moonbase-alpha"
            },
            "out3": {
              "type": "s3",
              "inputs": ["in"],
              "buffer_size": 100000,
              "gzip": true,
              "bucket": "asdf",
              "key_prefix": "logs/",
              "region": "us-east-1",
              "endpoint": "http://example.com/"
            }
          }
        }
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
        {
          "sources": {
            "in": {
              "type": "splunk",
              "address": "127.0.0.1:1235"
            }
          },
          "transforms": {
            "sampler": {
              "type": "sampler",
              "inputs": [],
              "rate": 10,
              "pass_list": ["error"]
            }
          },
          "sinks": {
            "out": {
              "type": "splunk_tcp",
              "inputs": [],
              "address": "127.0.0.1:9999"
            }
          }
        }
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
