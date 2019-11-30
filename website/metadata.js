module.exports = {
  "installation": {
    "containers": [
      {
        "archs": [
          "x86_64",
          "ARM64",
          "ARMv7"
        ],
        "id": "docker",
        "name": "Docker",
        "oss": [
          "Linux",
          "MacOS"
        ]
      }
    ],
    "downloads": [
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-unknown-linux-musl.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (x86_64)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "ARM64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-aarch64-unknown-linux-musl.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (ARM64)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "ARMv7",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-armv7-unknown-linux-musleabihf.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (ARMv7)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-apple-darwin.tar.gz",
        "file_type": "tar.gz",
        "name": "MacOS (x86_64)",
        "os": "MacOS",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-pc-windows-msvc.zip",
        "file_type": "zip",
        "name": "Windows (x86_64, 7+)",
        "os": "Windows",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-amd64.deb",
        "file_type": "deb",
        "name": "Deb (x86_64)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "ARM64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-arm64.deb",
        "file_type": "deb",
        "name": "Deb (ARM64)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "ARMv7",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-armhf.deb",
        "file_type": "deb",
        "name": "Deb (ARMv7)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64.rpm",
        "file_type": "rpm",
        "name": "RPM (x86_64)",
        "os": "Linux",
        "package_manager": "RPM",
        "type": "package"
      }
    ],
    "operating_systems": [
      {
        "id": "amazon-linux",
        "name": "Amazon Linux",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "centos",
        "name": "CentOS",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "debian",
        "name": "Debian",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "macos",
        "name": "MacOS",
        "os": "Linux",
        "package_manager": "Homebrew"
      },
      {
        "id": "raspberry-pi",
        "name": "Raspberry Pi",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "rhel",
        "name": "RHEL",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "ubuntu",
        "name": "Ubuntu",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "windows",
        "name": "Windows",
        "os": "Windows"
      }
    ],
    "package_managers": [
      {
        "archs": [
          "x86_64",
          "ARM64",
          "ARMv7"
        ],
        "id": "dpkg",
        "name": "DPKG"
      },
      {
        "archs": [
          "x86_64"
        ],
        "id": "homebrew",
        "name": "Homebrew"
      },
      {
        "archs": [
          "x86_64"
        ],
        "id": "rpm",
        "name": "RPM"
      }
    ]
  },
  "latest_post": {
    "author": "Luke",
    "date": "2019-06-28",
    "id": "introducing-vector",
    "path": "website/blog/2019-06-28-introducing-vector.md",
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
      "path": "website/blog/2019-06-28-introducing-vector.md",
      "permalink": "https://vector.dev/blog/introducing-vector",
      "tags": [
        "announcement"
      ],
      "title": "Introducing Vector"
    }
  ],
  "sinks": {
    "aws_cloudwatch_logs": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_cloudwatch_logs_sink",
      "name": "aws_cloudwatch_logs",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "AWS",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "datadog_metrics": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "datadog_metrics_sink",
      "name": "datadog_metrics",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "Datadog",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "Elastic",
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "Confluent",
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": "Splunk",
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    }
  },
  "sources": {
    "docker": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "docker_source",
      "name": "docker",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [
        "macos",
        "windows"
      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "udp": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "udp_source",
      "name": "udp",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    }
  },
  "transforms": {
    "add_fields": {
      "beta": false,
      "delivery_guarantee": null,
      "event_types": [
        "log"
      ],
      "function_category": "shape",
      "id": "add_fields_transform",
      "name": "add_fields",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
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
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    }
  }
};