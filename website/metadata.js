module.exports = {
  "installation": {
    "containers": [
      {
        "name": "Docker",
        "id": "docker"
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
  "latest_release": {
    "date": "2019-10-09",
    "downloads": {
      "vector-amd64.deb": "https://github.com/timberio/vector/releases/download/v0.5.0/vector-amd64.deb",
      "vector-x86_64-apple-darwin.tar.gz": "https://github.com/timberio/vector/releases/download/v0.5.0/vector-x86_64-apple-darwin.tar.gz",
      "vector-x86_64-unknown-linux-musl.tar.gz": "https://github.com/timberio/vector/releases/download/v0.5.0/vector-x86_64-unknown-linux-musl.tar.gz",
      "vector-x86_64.rpm": "https://github.com/timberio/vector/releases/download/v0.5.0/vector-x86_64.rpm"
    },
    "last_version": "0.4.0",
    "version": "0.5.0"
  },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The address to bind the socket to.",
          "display": null,
          "enum": null,
          "examples": [
            "0.0.0.0:9000"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "host_key": {
          "name": "host_key",
          "category": "Context",
          "default": "host",
          "description": "The key name added to each event representing the current host.",
          "display": null,
          "enum": null,
          "examples": [
            "host"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "max_length": {
          "name": "max_length",
          "category": "General",
          "default": 102400,
          "description": "The maximum bytes size of incoming messages before they are discarded.",
          "display": null,
          "enum": null,
          "examples": [
            102400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `udp`.",
          "display": null,
          "enum": {
            "udp": "The name of this component"
          },
          "examples": [
            "udp"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "max_length": {
          "name": "max_length",
          "category": "General",
          "default": 102400,
          "description": "The maxiumum bytes size of a message before it is discarded.",
          "display": null,
          "enum": null,
          "examples": [
            102400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "host_key": {
          "name": "host_key",
          "category": "Context",
          "default": "host",
          "description": "The key name added to each event representing the current host.",
          "display": null,
          "enum": null,
          "examples": [
            "host"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `stdin`.",
          "display": null,
          "enum": {
            "stdin": "The name of this component"
          },
          "examples": [
            "stdin"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "include_containers": {
          "name": "include_containers",
          "category": "General",
          "default": null,
          "description": "A list of container ids to match against when filtering running containers. This will attempt to match the container id from the beginning meaning you do not need to include the whole id but just the first few characters. If no containers ids are provided, all containers will be included.",
          "display": null,
          "enum": null,
          "examples": [
            "ffd2bc2cb74a"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "include_labels": {
          "name": "include_labels",
          "category": "General",
          "default": null,
          "description": " A list of container object labels to match against when filtering running containers. This should follow the described label's synatx in [docker object labels docs][urls.docker_object_labels]. ",
          "display": null,
          "enum": null,
          "examples": [
            "label_key=label_value"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `docker`.",
          "display": null,
          "enum": {
            "docker": "The name of this component"
          },
          "examples": [
            "docker"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The TCP address to listen for connections on, or \"systemd#N\" to use the Nth socket passed by systemd socket activation. ",
          "display": null,
          "enum": null,
          "examples": [
            "0.0.0.0:9000",
            "systemd",
            "systemd#1"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "shutdown_timeout_secs": {
          "name": "shutdown_timeout_secs",
          "category": "General",
          "default": 30,
          "description": "The timeout before a connection is forcefully closed during shutdown.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `vector`.",
          "display": null,
          "enum": {
            "vector": "The name of this component"
          },
          "examples": [
            "vector"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "UDP socket address to bind to.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.1:8126"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `statsd`.",
          "display": null,
          "enum": {
            "statsd": "The name of this component"
          },
          "examples": [
            "statsd"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "bootstrap_servers": {
          "name": "bootstrap_servers",
          "category": "General",
          "default": null,
          "description": "A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a \"bootstrap\" Kafka cluster that a Kafka client connects to initially to bootstrap itself.",
          "display": null,
          "enum": null,
          "examples": [
            "10.14.22.123:9092,10.14.23.332:9092"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "topics": {
          "name": "topics",
          "category": "General",
          "default": null,
          "description": "The Kafka topics names to read events from. Regex is supported if the topic begins with `^`.\n",
          "display": null,
          "enum": null,
          "examples": [
            [
              "topic-1",
              "topic-2",
              "^(prefix1|prefix2)-.+"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "group_id": {
          "name": "group_id",
          "category": "General",
          "default": null,
          "description": "The consumer group name to be used to consume events from Kafka.\n",
          "display": null,
          "enum": null,
          "examples": [
            "consumer-group-name"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "key_field": {
          "name": "key_field",
          "category": "General",
          "default": null,
          "description": "The log field name to use for the topic key. If unspecified, the key would not be added to the log event. If the message has null key, then this field would not be added to the log event.",
          "display": null,
          "enum": null,
          "examples": [
            "user_id"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "auto_offset_reset": {
          "name": "auto_offset_reset",
          "category": "General",
          "default": "largest",
          "description": "If offsets for consumer group do not exist, set them using this strategy. [librdkafka documentation][urls.lib_rdkafka_config] for `auto.offset.reset` option for explanation.",
          "display": null,
          "enum": null,
          "examples": [
            "smallest",
            "earliest",
            "beginning",
            "largest",
            "latest",
            "end",
            "error"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "session_timeout_ms": {
          "name": "session_timeout_ms",
          "category": "General",
          "default": 10000,
          "description": "The Kafka session timeout in milliseconds.\n",
          "display": null,
          "enum": null,
          "examples": [
            5000,
            10000
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "milliseconds"
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `kafka`.",
          "display": null,
          "enum": {
            "kafka": "The name of this component"
          },
          "examples": [
            "kafka"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "current_runtime_only": {
          "name": "current_runtime_only",
          "category": "General",
          "default": true,
          "description": "Include only entries from the current runtime (boot)",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "data_dir": {
          "name": "data_dir",
          "category": "General",
          "default": null,
          "description": "The directory used to persist the journal checkpoint position. By default, the global `data_dir` is used. Please make sure the Vector project has write permissions to this dir. ",
          "display": null,
          "enum": null,
          "examples": [
            "/var/lib/vector"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "local_only": {
          "name": "local_only",
          "category": "General",
          "default": true,
          "description": "Include only entries from the local system",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "units": {
          "name": "units",
          "category": "General",
          "default": [

          ],
          "description": "The list of units names to monitor. If empty or not present, all units are accepted. Unit names lacking a `\".\"` will have `\".service\"` appended to make them a valid service unit name.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "ntpd",
              "sysinit.target"
            ]
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `journald`.",
          "display": null,
          "enum": {
            "journald": "The name of this component"
          },
          "examples": [
            "journald"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "data_dir": {
          "name": "data_dir",
          "category": "General",
          "default": null,
          "description": "The directory used to persist file checkpoint positions. By default, the [global `data_dir` option][docs.configuration#data_dir] is used. Please make sure the Vector project has write permissions to this dir.",
          "display": null,
          "enum": null,
          "examples": [
            "/var/lib/vector"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "include": {
          "name": "include",
          "category": "General",
          "default": null,
          "description": "Array of file patterns to include. [Globbing](#globbing) is supported.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "/var/log/nginx/*.log"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "exclude": {
          "name": "exclude",
          "category": "General",
          "default": null,
          "description": "Array of file patterns to exclude. [Globbing](#globbing) is supported. *Takes precedence over the [`include` option](#include).*",
          "display": null,
          "enum": null,
          "examples": [
            [
              "/var/log/nginx/*.[0-9]*.log"
            ]
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "file_key": {
          "name": "file_key",
          "category": "Context",
          "default": "file",
          "description": "The key name added to each event with the full path of the file.",
          "display": null,
          "enum": null,
          "examples": [
            "file"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "glob_minimum_cooldown": {
          "name": "glob_minimum_cooldown",
          "category": "General",
          "default": 1000,
          "description": "Delay between file discovery calls. This controls the interval at which Vector searches for files.",
          "display": null,
          "enum": null,
          "examples": [
            1000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "milliseconds"
        },
        "host_key": {
          "name": "host_key",
          "category": "Context",
          "default": "host",
          "description": "The key name added to each event representing the current host.",
          "display": null,
          "enum": null,
          "examples": [
            "host"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "ignore_older": {
          "name": "ignore_older",
          "category": "General",
          "default": null,
          "description": "Ignore files with a data modification date that does not exceed this age.",
          "display": null,
          "enum": null,
          "examples": [
            86400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "max_line_bytes": {
          "name": "max_line_bytes",
          "category": "General",
          "default": 102400,
          "description": "The maximum number of a bytes a line can contain before being discarded. This protects against malformed lines or tailing incorrect files.",
          "display": null,
          "enum": null,
          "examples": [
            102400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "start_at_beginning": {
          "name": "start_at_beginning",
          "category": "General",
          "default": false,
          "description": "When `true` Vector will read from the beginning of new files, when `false` Vector will only read new data added to the file.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "fingerprinting": {
          "name": "fingerprinting",
          "category": "Fingerprinting",
          "default": null,
          "description": "Configuration for how the file source should identify files.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393bc5758>",
            "#<Option:0x00007fa393bc5578>",
            "#<Option:0x00007fa393bc5348>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "message_start_indicator": {
          "name": "message_start_indicator",
          "category": "Multi-line",
          "default": null,
          "description": "When present, Vector will aggregate multiple lines into a single event, using this pattern as the indicator that the previous lines should be flushed and a new event started. The pattern will be matched against entire lines as a regular expression, so remember to anchor as appropriate.",
          "display": null,
          "enum": null,
          "examples": [
            "^(INFO|ERROR)"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "multi_line_timeout": {
          "name": "multi_line_timeout",
          "category": "Multi-line",
          "default": 1000,
          "description": "When `message_start_indicator` is present, this sets the amount of time Vector will buffer lines into a single event before flushing, regardless of whether or not it has seen a line indicating the start of a new message.",
          "display": null,
          "enum": null,
          "examples": [
            1000
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "milliseconds"
        },
        "max_read_bytes": {
          "name": "max_read_bytes",
          "category": "Priority",
          "default": 2048,
          "description": "An approximate limit on the amount of data read from a single file at a given time.",
          "display": null,
          "enum": null,
          "examples": [
            2048
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "oldest_first": {
          "name": "oldest_first",
          "category": "Priority",
          "default": false,
          "description": "Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from younger files.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `file`.",
          "display": null,
          "enum": {
            "file": "The name of this component"
          },
          "examples": [
            "file"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The TCP or UDP address to listen for connections on, or \"systemd#N\" to use the Nth socket passed by systemd socket activation. ",
          "display": null,
          "enum": null,
          "examples": [
            "0.0.0.0:9000",
            "systemd",
            "systemd#2"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": {
            "mode": [
              "tcp",
              "udp"
            ]
          },
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "host_key": {
          "name": "host_key",
          "category": "Context",
          "default": "host",
          "description": "The key name added to each event representing the current host.",
          "display": null,
          "enum": null,
          "examples": [
            "host"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "max_length": {
          "name": "max_length",
          "category": "General",
          "default": 102400,
          "description": "The maximum bytes size of incoming messages before they are discarded.",
          "display": null,
          "enum": null,
          "examples": [
            102400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "mode": {
          "name": "mode",
          "category": "General",
          "default": null,
          "description": "The input mode.",
          "display": null,
          "enum": {
            "tcp": "Read incoming Syslog data over the TCP protocol.",
            "udp": "Read incoming Syslog data over the UDP protocol.",
            "unix": "Read uncoming Syslog data through a Unix socker."
          },
          "examples": [
            "tcp",
            "udp",
            "unix"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "path": {
          "name": "path",
          "category": "General",
          "default": null,
          "description": "The unix socket path. *This should be absolute path.*\n",
          "display": null,
          "enum": null,
          "examples": [
            "/path/to/socket"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": {
            "mode": "unix"
          },
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `syslog`.",
          "display": null,
          "enum": {
            "syslog": "The name of this component"
          },
          "examples": [
            "syslog"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The address to listen for connections on, or \"systemd#N\" to use the Nth socket passed by systemd socket activation. ",
          "display": null,
          "enum": null,
          "examples": [
            "0.0.0.0:9000",
            "systemd",
            "systemd#3"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "host_key": {
          "name": "host_key",
          "category": "Context",
          "default": "host",
          "description": "The key name added to each event representing the current host.",
          "display": null,
          "enum": null,
          "examples": [
            "host"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "max_length": {
          "name": "max_length",
          "category": "General",
          "default": 102400,
          "description": "The maximum bytes size of incoming messages before they are discarded.",
          "display": null,
          "enum": null,
          "examples": [
            102400
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "shutdown_timeout_secs": {
          "name": "shutdown_timeout_secs",
          "category": "General",
          "default": 30,
          "description": "The timeout before a connection is forcefully closed during shutdown.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `tcp`.",
          "display": null,
          "enum": {
            "tcp": "The name of this component"
          },
          "examples": [
            "tcp"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "metrics": {
          "name": "metrics",
          "category": "Metrics",
          "default": null,
          "description": "A table of key/value pairs representing the keys to be added to the event.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": false,
          "options": [
            "#<Option:0x00007fa393bdcbd8>",
            "#<Option:0x00007fa393bdc9f8>",
            "#<Option:0x00007fa393bdc868>",
            "#<Option:0x00007fa393bdc660>",
            "#<Option:0x00007fa393bdc4d0>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[table]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `log_to_metric`.",
          "display": null,
          "enum": {
            "log_to_metric": "The name of this component"
          },
          "examples": [
            "log_to_metric"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "field": {
          "name": "field",
          "category": "General",
          "default": "message",
          "description": "The field to apply the split on.",
          "display": null,
          "enum": null,
          "examples": [
            "message"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "field_names": {
          "name": "field_names",
          "category": "General",
          "default": null,
          "description": "The field names assigned to the resulting tokens, in order.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "timestamp",
              "level",
              "message"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "separator": {
          "name": "separator",
          "category": "General",
          "default": "whitespace",
          "description": "The separator to split the field on. If no separator is given, it will split on whitespace.",
          "display": null,
          "enum": null,
          "examples": [
            ","
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "drop_field": {
          "name": "drop_field",
          "category": "General",
          "default": true,
          "description": "If `true` the `field` will be dropped after parsing.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `split`.",
          "display": null,
          "enum": {
            "split": "The name of this component"
          },
          "examples": [
            "split"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "types": {
          "name": "types",
          "category": "Types",
          "default": null,
          "description": "Key/Value pairs representing mapped log field types.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393be4658>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "drop_invalid": {
          "name": "drop_invalid",
          "category": "General",
          "default": null,
          "description": "If `true` events with invalid JSON will be dropped, otherwise the event will be kept and passed through.",
          "display": null,
          "enum": null,
          "examples": [
            true
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "field": {
          "name": "field",
          "category": "General",
          "default": "message",
          "description": "The log field to decode as JSON. Must be a `string` value type.",
          "display": null,
          "enum": null,
          "examples": [
            "message"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `json_parser`.",
          "display": null,
          "enum": {
            "json_parser": "The name of this component"
          },
          "examples": [
            "json_parser"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "field": {
          "name": "field",
          "category": "General",
          "default": null,
          "description": "The target log field to compare against the `value`.",
          "display": null,
          "enum": null,
          "examples": [
            "file"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "value": {
          "name": "value",
          "category": "General",
          "default": null,
          "description": "If the value of the specified `field` matches this value then the event will be permitted, otherwise it is dropped.",
          "display": null,
          "enum": null,
          "examples": [
            "/var/log/nginx.log"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `field_filter`.",
          "display": null,
          "enum": {
            "field_filter": "The name of this component"
          },
          "examples": [
            "field_filter"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "fields": {
          "name": "fields",
          "category": "General",
          "default": null,
          "description": "The log field names to drop.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "field1",
              "field2"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `remove_fields`.",
          "display": null,
          "enum": {
            "remove_fields": "The name of this component"
          },
          "examples": [
            "remove_fields"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "drop_field": {
          "name": "drop_field",
          "category": "General",
          "default": true,
          "description": "If the specified `field` should be dropped (removed) after parsing.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "field": {
          "name": "field",
          "category": "General",
          "default": "message",
          "description": "The log field to parse.",
          "display": null,
          "enum": null,
          "examples": [
            "message"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "regex": {
          "name": "regex",
          "category": "General",
          "default": null,
          "description": "The Regular Expression to apply. Do not inlcude the leading or trailing `/`.",
          "display": null,
          "enum": null,
          "examples": [
            "^(?P<timestamp>.*) (?P<level>\\w*) (?P<message>.*)$"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `regex_parser`.",
          "display": null,
          "enum": {
            "regex_parser": "The name of this component"
          },
          "examples": [
            "regex_parser"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "types": {
          "name": "types",
          "category": "Types",
          "default": null,
          "description": "Key/Value pairs representing mapped log field types.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393bf5958>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "tags": {
          "name": "tags",
          "category": "Tags",
          "default": null,
          "description": "A table of key/value pairs representing the tags to be added to the metric.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": false,
          "options": [
            "#<Option:0x00007fa393bf4d00>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `add_tags`.",
          "display": null,
          "enum": {
            "add_tags": "The name of this component"
          },
          "examples": [
            "add_tags"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `coercer`.",
          "display": null,
          "enum": {
            "coercer": "The name of this component"
          },
          "examples": [
            "coercer"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "types": {
          "name": "types",
          "category": "Types",
          "default": null,
          "description": "Key/Value pairs representing mapped log field types.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393bfe328>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "fields": {
          "name": "fields",
          "category": "Fields",
          "default": null,
          "description": "A table of key/value pairs representing the keys to be added to the event.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": false,
          "options": [
            "#<Option:0x00007fa393bfd6d0>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `add_fields`.",
          "display": null,
          "enum": {
            "add_fields": "The name of this component"
          },
          "examples": [
            "add_fields"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "source": {
          "name": "source",
          "category": "General",
          "default": null,
          "description": "The inline Lua source to evaluate.",
          "display": null,
          "enum": null,
          "examples": [
            "require(\"script\") # a `script.lua` file must be in your `search_dirs`\n\nif event[\"host\"] == nil then\n  local f = io.popen (\"/bin/hostname\")\n  local hostname = f:read(\"*a\") or \"\"\n  f:close()\n  hostname = string.gsub(hostname, \"\\n$\", \"\")\n  event[\"host\"] = hostname\nend"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "search_dirs": {
          "name": "search_dirs",
          "category": "General",
          "default": null,
          "description": "A list of directories search when loading a Lua file via the `require` function.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "/etc/vector/lua"
            ]
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `lua`.",
          "display": null,
          "enum": {
            "lua": "The name of this component"
          },
          "examples": [
            "lua"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "field": {
          "name": "field",
          "category": "General",
          "default": "message",
          "description": "The log field to tokenize.",
          "display": null,
          "enum": null,
          "examples": [
            "message"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "field_names": {
          "name": "field_names",
          "category": "General",
          "default": null,
          "description": "The log field names assigned to the resulting tokens, in order.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "timestamp",
              "level",
              "message"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "drop_field": {
          "name": "drop_field",
          "category": "General",
          "default": true,
          "description": "If `true` the `field` will be dropped after parsing.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `tokenizer`.",
          "display": null,
          "enum": {
            "tokenizer": "The name of this component"
          },
          "examples": [
            "tokenizer"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "types": {
          "name": "types",
          "category": "Types",
          "default": null,
          "description": "Key/Value pairs representing mapped log field types.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c04908>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "drop_field": {
          "name": "drop_field",
          "category": "General",
          "default": true,
          "description": "If `true` will drop the specified `field` after parsing.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "field": {
          "name": "field",
          "category": "General",
          "default": "message",
          "description": "The log field to execute the `pattern` against. Must be a `string` value.",
          "display": null,
          "enum": null,
          "examples": [
            "message"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "pattern": {
          "name": "pattern",
          "category": "General",
          "default": null,
          "description": "The [Grok pattern][urls.grok_patterns]",
          "display": null,
          "enum": null,
          "examples": [
            "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `grok_parser`.",
          "display": null,
          "enum": {
            "grok_parser": "The name of this component"
          },
          "examples": [
            "grok_parser"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "types": {
          "name": "types",
          "category": "Types",
          "default": null,
          "description": "Key/Value pairs representing mapped log field types.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c0ddc8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "pass_list": {
          "name": "pass_list",
          "category": "General",
          "default": null,
          "description": "A list of regular expression patterns to exclude events from sampling. If an event's `\"message\"` key matches _any_ of these patterns it will _not_ be sampled.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "[error]",
              "field2"
            ]
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "rate": {
          "name": "rate",
          "category": "General",
          "default": null,
          "description": "The rate at which events will be forwarded, expressed as 1/N. For example, `rate = 10` means 1 out of every 10 events will be forwarded and the rest will be dropped.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `sampler`.",
          "display": null,
          "enum": {
            "sampler": "The name of this component"
          },
          "examples": [
            "sampler"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "tags": {
          "name": "tags",
          "category": "General",
          "default": null,
          "description": "The tag names to drop.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "tag1",
              "tag2"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `remove_tags`.",
          "display": null,
          "enum": {
            "remove_tags": "The name of this component"
          },
          "examples": [
            "remove_tags"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        }
      },
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
      "options": {
        "namespace": {
          "name": "namespace",
          "category": "General",
          "default": null,
          "description": "A prefix that will be added to all metric names.",
          "display": null,
          "enum": null,
          "examples": [
            "service"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "host": {
          "name": "host",
          "category": "General",
          "default": "https://api.datadoghq.com",
          "description": "Datadog endpoint to send metrics to.",
          "display": null,
          "enum": null,
          "examples": [
            "https://api.datadoghq.com",
            "https://api.datadoghq.eu"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "api_key": {
          "name": "api_key",
          "category": "General",
          "default": null,
          "description": "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)",
          "display": null,
          "enum": null,
          "examples": [
            "3111111111111111aaaaaaaaaaaaaaaa"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `datadog_metrics`.",
          "display": null,
          "enum": {
            "datadog_metrics": "The name of this component"
          },
          "examples": [
            "datadog_metrics"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 20,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            20
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 60,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            60
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "print_amount": {
          "name": "print_amount",
          "category": "General",
          "default": null,
          "description": "The number of events that must be received in order to print a summary of activity.",
          "display": null,
          "enum": null,
          "examples": [
            1000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `blackhole`.",
          "display": null,
          "enum": {
            "blackhole": "The name of this component"
          },
          "examples": [
            "blackhole"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        }
      },
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
      "options": {
        "bucket": {
          "name": "bucket",
          "category": "General",
          "default": null,
          "description": "The S3 bucket name. Do not include a leading `s3://` or a trailing `/`.",
          "display": null,
          "enum": null,
          "examples": [
            "my-bucket"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "filename_append_uuid": {
          "name": "filename_append_uuid",
          "category": "Object Names",
          "default": true,
          "description": "Whether or not to append a UUID v4 token to the end of the file. This ensures there are no name collisions high volume use cases.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "filename_extension": {
          "name": "filename_extension",
          "category": "Object Names",
          "default": "log",
          "description": "The extension to use in the object name.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "filename_time_format": {
          "name": "filename_time_format",
          "category": "Object Names",
          "default": "%s",
          "description": "The format of the resulting object file name. [`strftime` specifiers][urls.strftime_specifiers] are supported.",
          "display": null,
          "enum": null,
          "examples": [
            "%s"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "key_prefix": {
          "name": "key_prefix",
          "category": "Object Names",
          "default": "date=%F",
          "description": "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root S3 \"folder\".",
          "display": null,
          "enum": null,
          "examples": [
            "date=%F/",
            "date=%F/hour=%H/",
            "year=%Y/month=%m/day=%d/",
            "application_id={{ application_id }}/date=%F/"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": true,
          "relevant_when": null,
          "templateable": true,
          "type": "string",
          "unit": null
        },
        "region": {
          "name": "region",
          "category": "General",
          "default": null,
          "description": "The [AWS region][urls.aws_s3_regions] of the target S3 bucket.",
          "display": null,
          "enum": null,
          "examples": [
            "us-east-1"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `aws_s3`.",
          "display": null,
          "enum": {
            "aws_s3": "The name of this component"
          },
          "examples": [
            "aws_s3"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "endpoint",
          "category": "General",
          "default": null,
          "description": "Custom endpoint for use with AWS-compatible services.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.0:5000"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c24488>",
            "#<Option:0x00007fa393c242a8>",
            "#<Option:0x00007fa393c240c8>",
            "#<Option:0x00007fa393c2fec8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 10490000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            10490000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 300,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            300
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 30,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The downstream Vector address.",
          "display": null,
          "enum": null,
          "examples": [
            "92.12.333.224:5000"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `vector`.",
          "display": null,
          "enum": {
            "vector": "The name of this component"
          },
          "examples": [
            "vector"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c36fe8>",
            "#<Option:0x00007fa393c36e08>",
            "#<Option:0x00007fa393c36c28>",
            "#<Option:0x00007fa393c36a98>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": "127.0.0.1:8125",
          "description": "The UDP socket address to send stats to.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.1:8125"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "namespace": {
          "name": "namespace",
          "category": "General",
          "default": null,
          "description": "A prefix that will be added to all metric names.",
          "display": null,
          "enum": null,
          "examples": [
            "service"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `statsd`.",
          "display": null,
          "enum": {
            "statsd": "The name of this component"
          },
          "examples": [
            "statsd"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        }
      },
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
      "options": {
        "group_name": {
          "name": "group_name",
          "category": "General",
          "default": null,
          "description": "The [group name][urls.aws_cw_logs_group_name] of the target CloudWatch Logs stream.",
          "display": null,
          "enum": null,
          "examples": [
            "{{ file }}",
            "ec2/{{ instance_id }}",
            "group-name"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": true,
          "relevant_when": null,
          "templateable": true,
          "type": "string",
          "unit": null
        },
        "region": {
          "name": "region",
          "category": "General",
          "default": null,
          "description": "The [AWS region][urls.aws_cw_logs_regions] of the target CloudWatch Logs stream resides.",
          "display": null,
          "enum": null,
          "examples": [
            "us-east-1"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "stream_name": {
          "name": "stream_name",
          "category": "General",
          "default": null,
          "description": "The [stream name][urls.aws_cw_logs_stream_name] of the target CloudWatch Logs stream.",
          "display": null,
          "enum": null,
          "examples": [
            "{{ instance_id }}",
            "%Y-%m-%d",
            "stream-name"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": true,
          "relevant_when": null,
          "templateable": true,
          "type": "string",
          "unit": null
        },
        "create_missing_group": {
          "name": "create_missing_group",
          "category": "General",
          "default": true,
          "description": "Dynamically create a [log group][urls.aws_cw_logs_group_name] if it does not already exist. This will ignore `create_missing_stream` directly after creating the group and will create the first stream. ",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "create_missing_stream": {
          "name": "create_missing_stream",
          "category": "General",
          "default": true,
          "description": "Dynamically create a [log stream][urls.aws_cw_logs_stream_name] if it does not already exist.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `aws_cloudwatch_logs`.",
          "display": null,
          "enum": {
            "aws_cloudwatch_logs": "The name of this component"
          },
          "examples": [
            "aws_cloudwatch_logs"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "endpoint",
          "category": "General",
          "default": null,
          "description": "Custom endpoint for use with AWS-compatible services.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.0:5000"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c3c470>",
            "#<Option:0x00007fa393c3c290>",
            "#<Option:0x00007fa393c3c0b0>",
            "#<Option:0x00007fa393c47ed8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 1049000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1049000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 30,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "doc_type": {
          "name": "doc_type",
          "category": "General",
          "default": "_doc",
          "description": "The `doc_type` for your index data. This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to set this option since Elasticsearch has removed it.",
          "display": null,
          "enum": null,
          "examples": [
            "_doc"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "host": {
          "name": "host",
          "category": "General",
          "default": null,
          "description": "The host of your Elasticsearch cluster. This should be the full URL as shown in the example.",
          "display": null,
          "enum": null,
          "examples": [
            "http://10.24.32.122:9000"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "index": {
          "name": "index",
          "category": "General",
          "default": "vector-%F",
          "description": "Index name to write events to.",
          "display": null,
          "enum": null,
          "examples": [
            "vector-%Y-%m-%d",
            "application-{{ application_id }}-%Y-%m-%d"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": true,
          "type": "string",
          "unit": null
        },
        "basic_auth": {
          "name": "basic_auth",
          "category": "Basic auth",
          "default": null,
          "description": "Options for basic authentication.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c4fe80>",
            "#<Option:0x00007fa393c4fcf0>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "headers": {
          "name": "headers",
          "category": "Headers",
          "default": null,
          "description": "Options for custom headers.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c4f278>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "provider": {
          "name": "provider",
          "category": "General",
          "default": "default",
          "description": "The provider of the Elasticsearch service. This is used to properly authenticate with the Elasticsearch cluster. For example, authentication for [AWS Elasticsearch Service][urls.aws_elasticsearch] requires that we obtain AWS credentials to properly sign the request.",
          "display": null,
          "enum": {
            "default": "A generic Elasticsearch provider.",
            "aws": "The [AWS Elasticsearch Service][urls.aws_elasticsearch]."
          },
          "examples": [
            "default",
            "aws"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "query": {
          "name": "query",
          "category": "Query",
          "default": null,
          "description": "Custom parameters to Elasticsearch query string.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c4e508>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "region": {
          "name": "region",
          "category": "General",
          "default": null,
          "description": "When using the AWS provider, the [AWS region][urls.aws_elasticsearch_regions] of the target Elasticsearch instance.",
          "display": null,
          "enum": null,
          "examples": [
            "us-east-1"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `elasticsearch`.",
          "display": null,
          "enum": {
            "elasticsearch": "The name of this component"
          },
          "examples": [
            "elasticsearch"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c57db0>",
            "#<Option:0x00007fa393c57bd0>",
            "#<Option:0x00007fa393c579c8>",
            "#<Option:0x00007fa393c57838>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c56a78>",
            "#<Option:0x00007fa393c568e8>",
            "#<Option:0x00007fa393c56758>",
            "#<Option:0x00007fa393c565c8>",
            "#<Option:0x00007fa393c56438>",
            "#<Option:0x00007fa393c56230>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 10490000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            10490000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 60,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            60
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "bootstrap_servers": {
          "name": "bootstrap_servers",
          "category": "General",
          "default": null,
          "description": "A list of host and port pairs that the Kafka client should contact to bootstrap its cluster metadata.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "10.14.22.123:9092",
              "10.14.23.332:9092"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "key_field": {
          "name": "key_field",
          "category": "General",
          "default": null,
          "description": "The log field name to use for the topic key. If unspecified, the key will be randomly generated. If the field does not exist on the log, a blank value will be used.",
          "display": null,
          "enum": null,
          "examples": [
            "user_id"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "topic": {
          "name": "topic",
          "category": "General",
          "default": null,
          "description": "The Kafka topic name to write events to.",
          "display": null,
          "enum": null,
          "examples": [
            "topic-1234"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `kafka`.",
          "display": null,
          "enum": {
            "kafka": "The name of this component"
          },
          "examples": [
            "kafka"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "json": "Each event is encoded into JSON and the payload is represented as a JSON array.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "json",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c5c5b8>",
            "#<Option:0x00007fa393c5c3d8>",
            "#<Option:0x00007fa393c5c1f8>",
            "#<Option:0x00007fa393c5c068>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c672b0>",
            "#<Option:0x00007fa393c670a8>",
            "#<Option:0x00007fa393c66f18>",
            "#<Option:0x00007fa393c66d88>",
            "#<Option:0x00007fa393c66bf8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "host": {
          "name": "host",
          "category": "General",
          "default": null,
          "description": "Your Splunk HEC host.",
          "display": null,
          "enum": null,
          "examples": [
            "my-splunk-host.com"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "token": {
          "name": "token",
          "category": "General",
          "default": null,
          "description": "Your Splunk HEC token.",
          "display": null,
          "enum": null,
          "examples": [
            "A94A8FE5CCB19BA61C4C08"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `splunk_hec`.",
          "display": null,
          "enum": {
            "splunk_hec": "The name of this component"
          },
          "examples": [
            "splunk_hec"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "ndjson": "Each event is encoded into JSON and the payload is new line delimited.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "ndjson",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c6fa28>",
            "#<Option:0x00007fa393c6f820>",
            "#<Option:0x00007fa393c6f618>",
            "#<Option:0x00007fa393c6f488>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c6e6c8>",
            "#<Option:0x00007fa393c6e538>",
            "#<Option:0x00007fa393c6e380>",
            "#<Option:0x00007fa393c6e1a0>",
            "#<Option:0x00007fa393c6dfe8>",
            "#<Option:0x00007fa393c6dd90>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 1049000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1049000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 10,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 60,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            60
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 10,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "target": {
          "name": "target",
          "category": "General",
          "default": "stdout",
          "description": "The [standard stream][urls.standard_streams] to write to.",
          "display": null,
          "enum": {
            "stdout": "Output will be written to [STDOUT][urls.stdout]",
            "stderr": "Output will be written to [STDERR][urls.stderr]"
          },
          "examples": [
            "stdout",
            "stderr"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `console`.",
          "display": null,
          "enum": {
            "console": "The name of this component"
          },
          "examples": [
            "console"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "json": "Each event is encoded into JSON and the payload is represented as a JSON array.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "json",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "partition_key_field": {
          "name": "partition_key_field",
          "category": "General",
          "default": null,
          "description": "The log field used as the Kinesis record's partition key value.",
          "display": null,
          "enum": null,
          "examples": [
            "user_id"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "region": {
          "name": "region",
          "category": "General",
          "default": null,
          "description": "The [AWS region][urls.aws_cw_logs_regions] of the target Kinesis stream resides.",
          "display": null,
          "enum": null,
          "examples": [
            "us-east-1"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "stream_name": {
          "name": "stream_name",
          "category": "General",
          "default": null,
          "description": "The [stream name][urls.aws_cw_logs_stream_name] of the target Kinesis Logs stream.",
          "display": null,
          "enum": null,
          "examples": [
            "my-stream"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `aws_kinesis_streams`.",
          "display": null,
          "enum": {
            "aws_kinesis_streams": "The name of this component"
          },
          "examples": [
            "aws_kinesis_streams"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "endpoint",
          "category": "General",
          "default": null,
          "description": "Custom endpoint for use with AWS-compatible services.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.0:5000"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c7d718>",
            "#<Option:0x00007fa393c7d538>",
            "#<Option:0x00007fa393c7d358>",
            "#<Option:0x00007fa393c7d1c8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 1049000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1049000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 30,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 5,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "host": {
          "name": "host",
          "category": "General",
          "default": null,
          "description": "The host url of the [Clickhouse][urls.clickhouse] server.",
          "display": null,
          "enum": null,
          "examples": [
            "http://localhost:8123"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "table": {
          "name": "table",
          "category": "General",
          "default": null,
          "description": "The table that data will be inserted into.",
          "display": null,
          "enum": null,
          "examples": [
            "mytable"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "database": {
          "name": "database",
          "category": "General",
          "default": null,
          "description": "The database that contains the stable that data will be inserted into.",
          "display": null,
          "enum": null,
          "examples": [
            "mydatabase"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "basic_auth": {
          "name": "basic_auth",
          "category": "Basic auth",
          "default": null,
          "description": "Options for basic authentication.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c853a0>",
            "#<Option:0x00007fa393c85210>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `clickhouse`.",
          "display": null,
          "enum": {
            "clickhouse": "The name of this component"
          },
          "examples": [
            "clickhouse"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "compression",
          "category": "requests",
          "default": "gzip",
          "description": "The compression strategy used to compress the encoded event data before outputting.",
          "display": null,
          "enum": {
            "gzip": "The payload will be compressed in [Gzip][urls.gzip] format before being sent."
          },
          "examples": [
            "gzip"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c8ea90>",
            "#<Option:0x00007fa393c8e8b0>",
            "#<Option:0x00007fa393c8e6d0>",
            "#<Option:0x00007fa393c8e540>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393c8d780>",
            "#<Option:0x00007fa393c8d5f0>",
            "#<Option:0x00007fa393c8d460>",
            "#<Option:0x00007fa393c8d2d0>",
            "#<Option:0x00007fa393c8d118>",
            "#<Option:0x00007fa393c8cf10>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 1049000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1049000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 1,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 30,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 5,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 9223372036854775807,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            9223372036854775807
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 9223372036854775807,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            9223372036854775807
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "path": {
          "name": "path",
          "category": "General",
          "default": null,
          "description": "File name to write events to.",
          "display": null,
          "enum": null,
          "examples": [
            "vector-%Y-%m-%d.log",
            "application-{{ application_id }}-%Y-%m-%d.log"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": true,
          "type": "string",
          "unit": null
        },
        "idle_timeout_secs": {
          "name": "idle_timeout_secs",
          "category": "General",
          "default": "30",
          "description": "The amount of time a file can be idle  and stay open. After not receiving any events for this timeout, the file will be flushed and closed.\n",
          "display": null,
          "enum": null,
          "examples": [
            "30"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `file`.",
          "display": null,
          "enum": {
            "file": "The name of this component"
          },
          "examples": [
            "file"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "ndjson": "Each event is encoded into JSON and the payload is new line delimited.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "ndjson",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "namespace": {
          "name": "namespace",
          "category": "General",
          "default": null,
          "description": "A [namespace](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/cloudwatch_concepts.html#Namespace) that will isolate different metrics from each other.",
          "display": null,
          "enum": null,
          "examples": [
            "service"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "region": {
          "name": "region",
          "category": "General",
          "default": null,
          "description": "The [AWS region][urls.aws_cw_metrics_regions] of the target CloudWatch stream resides.",
          "display": null,
          "enum": null,
          "examples": [
            "us-east-1"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `aws_cloudwatch_metrics`.",
          "display": null,
          "enum": {
            "aws_cloudwatch_metrics": "The name of this component"
          },
          "examples": [
            "aws_cloudwatch_metrics"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "endpoint",
          "category": "General",
          "default": null,
          "description": "Custom endpoint for use with AWS-compatible services.",
          "display": null,
          "enum": null,
          "examples": [
            "127.0.0.0:5000"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The TCP address.",
          "display": null,
          "enum": null,
          "examples": [
            "92.12.333.224:5000"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `tcp`.",
          "display": null,
          "enum": {
            "tcp": "The name of this component"
          },
          "examples": [
            "tcp"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "json": "Each event is encoded into JSON and the payload is represented as a JSON array.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "json",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393ca6b68>",
            "#<Option:0x00007fa393ca6910>",
            "#<Option:0x00007fa393ca66b8>",
            "#<Option:0x00007fa393ca6500>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393ca5470>",
            "#<Option:0x00007fa393ca5268>",
            "#<Option:0x00007fa393ca50d8>",
            "#<Option:0x00007fa393ca4f48>",
            "#<Option:0x00007fa393ca4db8>",
            "#<Option:0x00007fa393ca4c28>",
            "#<Option:0x00007fa393ca4a20>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        }
      },
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
      "options": {
        "basic_auth": {
          "name": "basic_auth",
          "category": "Basic auth",
          "default": null,
          "description": "Options for basic authentication.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393cafd58>",
            "#<Option:0x00007fa393cafbc8>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "headers": {
          "name": "headers",
          "category": "Headers",
          "default": null,
          "description": "Options for custom headers.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393caf178>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "healthcheck_uri": {
          "name": "healthcheck_uri",
          "category": "General",
          "default": null,
          "description": "A URI that Vector can request in order to determine the service health.",
          "display": null,
          "enum": null,
          "examples": [
            "https://10.22.212.22:9000/_health"
          ],
          "null": true,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "uri": {
          "name": "uri",
          "category": "General",
          "default": null,
          "description": "The full URI to make HTTP requests to. This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.",
          "display": null,
          "enum": null,
          "examples": [
            "https://10.22.212.22:9000/endpoint"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `http`.",
          "display": null,
          "enum": {
            "http": "The name of this component"
          },
          "examples": [
            "http"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        },
        "hostname": {
          "name": "encoding",
          "category": "requests",
          "default": null,
          "description": "The encoding format used to serialize the events before outputting.",
          "display": null,
          "enum": {
            "ndjson": "Each event is encoded into JSON and the payload is new line delimited.",
            "text": "Each event is encoded into text via the `message` key and the payload is new line delimited."
          },
          "examples": [
            "ndjson",
            "text"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buffer": {
          "name": "buffer",
          "category": "Buffer",
          "default": null,
          "description": "Configures the sink specific buffer.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393cb7d78>",
            "#<Option:0x00007fa393cb7b98>",
            "#<Option:0x00007fa393cb79b8>",
            "#<Option:0x00007fa393cb7828>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "tls": {
          "name": "tls",
          "category": "Tls",
          "default": null,
          "description": "Configures the TLS options for connections from this sink.",
          "display": null,
          "enum": null,
          "examples": [

          ],
          "null": true,
          "options": [
            "#<Option:0x00007fa393cb6a68>",
            "#<Option:0x00007fa393cb68d8>",
            "#<Option:0x00007fa393cb6720>",
            "#<Option:0x00007fa393cb6590>",
            "#<Option:0x00007fa393cb63d8>",
            "#<Option:0x00007fa393cb61d0>"
          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "table",
          "unit": null
        },
        "batch_size": {
          "name": "batch_size",
          "category": "Batching",
          "default": 1049000,
          "description": "The maximum size of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            1049000
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "bytes"
        },
        "batch_timeout": {
          "name": "batch_timeout",
          "category": "Batching",
          "default": 5,
          "description": "The maximum age of a batch before it is flushed.",
          "display": null,
          "enum": null,
          "examples": [
            5
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "request_in_flight_limit": {
          "name": "request_in_flight_limit",
          "category": "Requests",
          "default": 10,
          "description": "The maximum number of in-flight requests allowed at any given time.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "request_timeout_secs": {
          "name": "request_timeout_secs",
          "category": "Requests",
          "default": 30,
          "description": "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
          "display": null,
          "enum": null,
          "examples": [
            30
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_duration": {
          "name": "rate_limit_duration",
          "category": "Requests",
          "default": 1,
          "description": "The window used for the `request_rate_limit_num` option",
          "display": null,
          "enum": null,
          "examples": [
            1
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        },
        "rate_limit_num": {
          "name": "rate_limit_num",
          "category": "Requests",
          "default": 10,
          "description": "The maximum number of requests allowed within the `rate_limit_duration` window.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_attempts": {
          "name": "retry_attempts",
          "category": "Requests",
          "default": 10,
          "description": "The maximum number of retries to make for failed requests.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": null
        },
        "retry_backoff_secs": {
          "name": "retry_backoff_secs",
          "category": "Requests",
          "default": 10,
          "description": "The amount of time to wait before attempting a failed request again.",
          "display": null,
          "enum": null,
          "examples": [
            10
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "int",
          "unit": "seconds"
        }
      },
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
      "options": {
        "address": {
          "name": "address",
          "category": "General",
          "default": null,
          "description": "The address to expose for scraping.",
          "display": null,
          "enum": null,
          "examples": [
            "0.0.0.0:9598"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "namespace": {
          "name": "namespace",
          "category": "General",
          "default": null,
          "description": "A prefix that will be added to all metric names.\nIt should follow Prometheus [naming conventions][urls.prometheus_metric_naming].",
          "display": null,
          "enum": null,
          "examples": [
            "service"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "buckets": {
          "name": "buckets",
          "category": "General",
          "default": [
            0.005,
            0.01,
            0.025,
            0.05,
            0.1,
            0.25,
            0.5,
            1.0,
            2.5,
            5.0,
            10.0
          ],
          "description": "Default buckets to use for [histogram][docs.data-model.metric#histogram] metrics.",
          "display": null,
          "enum": null,
          "examples": [
            [
              0.005,
              0.01,
              0.025,
              0.05,
              0.1,
              0.25,
              0.5,
              1.0,
              2.5,
              5.0,
              10.0
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[float]",
          "unit": "seconds"
        },
        "type": {
          "name": "type",
          "category": "General",
          "default": null,
          "description": "The component type. This is a required field that tells Vector which component to use. The value _must_ be `prometheus`.",
          "display": null,
          "enum": {
            "prometheus": "The name of this component"
          },
          "examples": [
            "prometheus"
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "string",
          "unit": null
        },
        "inputs": {
          "name": "inputs",
          "category": "General",
          "default": null,
          "description": "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.configuration#composition] for more info.",
          "display": null,
          "enum": null,
          "examples": [
            [
              "my-source-id"
            ]
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "[string]",
          "unit": null
        },
        "healthcheck": {
          "name": "healthcheck",
          "category": "General",
          "default": true,
          "description": "Enables/disables the sink healthcheck upon start.",
          "display": null,
          "enum": null,
          "examples": [
            true,
            false
          ],
          "null": false,
          "options": [

          ],
          "partition_key": false,
          "relevant_when": null,
          "templateable": false,
          "type": "bool",
          "unit": null
        }
      },
      "service_provider": null,
      "status": "beta",
      "type": "sink"
    }
  }
};