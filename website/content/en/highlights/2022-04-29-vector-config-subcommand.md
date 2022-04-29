---
date: "2022-04-29"
title: "Vector Config Subcommand"
description: "A new subcommand to output configuration(s) as a standard JSON object"
authors: ["001wwang"]
pr_numbers: []
release: "0.22.0"
hide_on_release_notes: false
---

We've added a new subcommand `vector config` to help format and standardize your
configurations. This can be useful when checking configurations into version
control.

For example, with the following configuration,

```toml
[api]
enabled = true

[sources.in]
type = "demo_logs"
format = "syslog"
interval = 1.0

[sinks.out]
type = "blackhole"
inputs = ["in"]
```

running `vector config -c {path to configuration}` will output the following
JSON.

```jsonc
{
  "api": {
    "enabled": true
  },
  "sinks": {
    "out": {
      "inputs": [
        "in"
      ],
      "type": "blackhole"
    }
  },
  "sources": {
    "in": {
      "format": "syslog",
      "interval": 1.0,
      "type": "demo_logs"
    }
  }
}
```

If, on a whim, you decide to change the original configuration's ordering like
so,

```toml
[api]
enabled = true

[sinks.out]
inputs = ["in"]
type = "blackhole"

[sources.in]
type = "demo_logs"
interval = 1.0
format = "syslog"
```

`vector config` will continue to provide the same output. In other words, using
`vector config` to process a configuration allows you to to ignore stylistic
changes that don't affect the configuration's actual content. The
`--include-defaults` flag is also useful for documenting configuration values
provided as defaults when not explicitly configured. For the above
configuration, running `vector config -c {path to configuration}
--include-defaults` will output the following.

```jsonc
{
  "data_dir": "/var/lib/vector/",
  "api": {
    "enabled": true,
    "address": "127.0.0.1:8686",
    "playground": true
  },
  "schema": {
    "enabled": false
  },
  "enterprise": null,
  "healthchecks": {
    "enabled": true,
    "require_healthy": false
  },
  "enrichment_tables": {},
  "sources": {
    "in": {
      "type": "demo_logs",
      "interval": 1.0,
      "count": 9223372036854775807,
      "format": "syslog",
      "framing": {
        "method": "bytes"
      },
      "decoding": {
        "codec": "bytes"
      }
    }
  },
  "sinks": {
    "out": {
      "inputs": [
        "in"
      ],
      "healthcheck_uri": null,
      "healthcheck": {
        "enabled": true,
        "uri": null
      },
      "buffer": {
        "type": "memory",
        "max_events": 500,
        "when_full": "block"
      },
      "type": "blackhole",
      "print_interval_secs": 1,
      "rate": null
    }
  },
  "transforms": {},
  "tests": [],
  "provider": null
}
```
