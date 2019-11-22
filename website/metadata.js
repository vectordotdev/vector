module.exports = {
  "installation": {
    "containers": [
      {
        "name": "Docker",
        "id": "docker"
      }
    ],
    "downloads": [
      {
        "name": "Linux (x86_64 w/ MUSL)",
        "file_name": "vector-x86_64-unknown-linux-musl.tar.gz",
        "latest": true,
        "nightly": true
      },
      {
        "name": "Linux (ARM64 w/ MUSL)",
        "file_name": "vector-aarch64-unknown-linux-musl.tar.gz",
        "latest": false,
        "nightly": true
      },
      {
        "name": "MacOS (64-bit OSX)",
        "file_name": "vector-x86_64-apple-darwin.tar.gz",
        "latest": true,
        "nightly": true
      },
      {
        "name": "Deb",
        "file_name": "vector-amd64.deb",
        "latest": true,
        "nightly": true
      },
      {
        "name": "RPM",
        "file_name": "vector-x86_64.rpm",
        "latest": true,
        "nightly": true
      }
    ],
    "operating_systems": [
      {
        "name": "Amazon Linux",
        "id": "amazon-linux"
      },
      {
        "name": "CentOS",
        "id": "centos"
      },
      {
        "name": "Debian",
        "id": "debian"
      },
      {
        "name": "MacOS",
        "id": "macos"
      },
      {
        "name": "Raspberry Pi",
        "id": "raspberry-pi"
      },
      {
        "name": "RHEL",
        "id": "rhel"
      },
      {
        "name": "Ubuntu",
        "id": "ubuntu"
      }
    ],
    "package_managers": [
      {
        "name": "DPKG",
        "id": "dpkg"
      },
      {
        "name": "Homebrew",
        "id": "homebrew"
      },
      {
        "name": "RPM",
        "id": "rpm"
      }
    ]
  },
  "latest_post": {
    "author": "Luke",
    "date": "2019-06-28",
    "id": "introducing-vector",
    "path": "/Users/benjohnson/Code/timber/vector/website/blog/2019-06-28-introducing-vector.md",
    "permalink": "https://vector.dev/blog/introducing-vector",
    "tags": [
      "announcement"
    ],
    "title": "Introducing Vector"
  },
  "latest_release": {
    "date": "2019-10-09",
    "last_version": "0.4.0",
    "version": "0.5.0"
  },
  "posts": [
    {
      "author": "Luke",
      "date": "2019-06-28",
      "id": "introducing-vector",
      "path": "/Users/benjohnson/Code/timber/vector/website/blog/2019-06-28-introducing-vector.md",
      "permalink": "https://vector.dev/blog/introducing-vector",
      "tags": [
        "announcement"
      ],
      "title": "Introducing Vector"
    }
  ],
  "sources": {
    "udp": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "udp_source",
      "name": "udp",
      "service_provider": null,
      "status": "prod-ready",
      "type": "source"
    },
    "stdin": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "stdin_source",
      "name": "stdin",
      "service_provider": null,
      "status": "prod-ready",
      "type": "source"
    },
    "docker": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "docker_source",
      "name": "docker",
      "service_provider": null,
      "status": "beta",
      "type": "source"
    },
    "vector": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "proxy",
      "id": "vector_source",
      "name": "vector",
      "service_provider": null,
      "status": "beta",
      "type": "source"
    },
    "statsd": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "metric"
      ],
      "function_category": "receive",
      "id": "statsd_source",
      "name": "statsd",
      "service_provider": null,
      "status": "beta",
      "type": "source"
    },
    "kafka": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "kafka_source",
      "name": "kafka",
      "service_provider": null,
      "status": "beta",
      "type": "source"
    },
    "journald": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "journald_source",
      "name": "journald",
      "service_provider": null,
      "status": "beta",
      "type": "source"
    },
    "file": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "file_source",
      "name": "file",
      "service_provider": null,
      "status": "prod-ready",
      "type": "source"
    },
    "syslog": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "syslog_source",
      "name": "syslog",
      "service_provider": null,
      "status": "prod-ready",
      "type": "source"
    },
    "tcp": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "tcp_source",
      "name": "tcp",
      "service_provider": null,
      "status": "prod-ready",
      "type": "source"
    }
  },
  "transforms": {
    "log_to_metric": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "convert",
      "id": "log_to_metric_transform",
      "name": "log_to_metric",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "split": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "split_transform",
      "name": "split",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "json_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "json_parser_transform",
      "name": "json_parser",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "field_filter": {
      "beta": true,
      "delivery_guarantee": null,
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "filter",
      "id": "field_filter_transform",
      "name": "field_filter",
      "service_provider": null,
      "status": "beta",
      "type": "transform"
    },
    "remove_fields": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "shape",
      "id": "remove_fields_transform",
      "name": "remove_fields",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "regex_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "regex_parser_transform",
      "name": "regex_parser",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "add_tags": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "metric"
      ],
      "function_category": "shape",
      "id": "add_tags_transform",
      "name": "add_tags",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "coercer": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "coercer_transform",
      "name": "coercer",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "add_fields": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "shape",
      "id": "add_fields_transform",
      "name": "add_fields",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "lua": {
      "beta": true,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "program",
      "id": "lua_transform",
      "name": "lua",
      "service_provider": null,
      "status": "beta",
      "type": "transform"
    },
    "tokenizer": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "tokenizer_transform",
      "name": "tokenizer",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "grok_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "grok_parser_transform",
      "name": "grok_parser",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    },
    "sampler": {
      "beta": true,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "filter",
      "id": "sampler_transform",
      "name": "sampler",
      "service_provider": null,
      "status": "beta",
      "type": "transform"
    },
    "remove_tags": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "metric"
      ],
      "function_category": "shape",
      "id": "remove_tags_transform",
      "name": "remove_tags",
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform"
    }
  },
  "sinks": {
    "datadog_metrics": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "datadog_metrics_sink",
      "name": "datadog_metrics",
      "service_provider": "Datadog",
      "status": "beta",
      "type": "sink"
    },
    "blackhole": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "test",
      "id": "blackhole_sink",
      "name": "blackhole",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "aws_s3": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_s3_sink",
      "name": "aws_s3",
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink"
    },
    "vector": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "proxy",
      "id": "vector_sink",
      "name": "vector",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "statsd": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "statsd_sink",
      "name": "statsd",
      "service_provider": null,
      "status": "beta",
      "type": "sink"
    },
    "aws_cloudwatch_logs": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_cloudwatch_logs_sink",
      "name": "aws_cloudwatch_logs",
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink"
    },
    "elasticsearch": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "elasticsearch_sink",
      "name": "elasticsearch",
      "service_provider": "Elastic",
      "status": "beta",
      "type": "sink"
    },
    "kafka": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "kafka_sink",
      "name": "kafka",
      "service_provider": "Confluent",
      "status": "prod-ready",
      "type": "sink"
    },
    "splunk_hec": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "splunk_hec_sink",
      "name": "splunk_hec",
      "service_provider": "Splunk",
      "status": "prod-ready",
      "type": "sink"
    },
    "console": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "test",
      "id": "console_sink",
      "name": "console",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "aws_kinesis_streams": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_kinesis_streams_sink",
      "name": "aws_kinesis_streams",
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink"
    },
    "clickhouse": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "clickhouse_sink",
      "name": "clickhouse",
      "service_provider": null,
      "status": "beta",
      "type": "sink"
    },
    "file": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "file_sink",
      "name": "file",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "aws_cloudwatch_metrics": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "aws_cloudwatch_metrics_sink",
      "name": "aws_cloudwatch_metrics",
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink"
    },
    "tcp": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "tcp_sink",
      "name": "tcp",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "http": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "http_sink",
      "name": "http",
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink"
    },
    "prometheus": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "prometheus_sink",
      "name": "prometheus",
      "service_provider": null,
      "status": "beta",
      "type": "sink"
    }
  }
};