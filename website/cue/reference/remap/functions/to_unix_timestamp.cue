{
  "remap": {
    "functions": {
      "to_unix_timestamp": {
        "anchor": "to_unix_timestamp",
        "name": "to_unix_timestamp",
        "category": "Convert",
        "description": "Converts the `value` timestamp into a [Unix timestamp](https://en.wikipedia.org/wiki/Unix_time).\n\nReturns the number of seconds since the Unix epoch by default. To return the number in milliseconds or nanoseconds, set the `unit` argument to `milliseconds` or `nanoseconds`.",
        "arguments": [
          {
            "name": "value",
            "description": "The timestamp to convert into a Unix timestamp.",
            "required": true,
            "type": [
              "timestamp"
            ]
          },
          {
            "name": "unit",
            "description": "The time unit.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "seconds": "Express Unix time in seconds",
              "nanoseconds": "Express Unix time in nanoseconds",
              "milliseconds": "Express Unix time in milliseconds"
            },
            "default": "seconds"
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "internal_failure_reasons": [
          "`value` cannot be represented in nanoseconds. Result is too large or too small for a 64 bit integer."
        ],
        "examples": [
          {
            "title": "Convert to a Unix timestamp (seconds)",
            "source": "to_unix_timestamp(t'2021-01-01T00:00:00+00:00')",
            "return": 1609459200
          },
          {
            "title": "Convert to a Unix timestamp (milliseconds)",
            "source": "to_unix_timestamp(t'2021-01-01T00:00:00Z', unit: \"milliseconds\")",
            "return": 1609459200000
          },
          {
            "title": "Convert to a Unix timestamp (microseconds)",
            "source": "to_unix_timestamp(t'2021-01-01T00:00:00Z', unit: \"microseconds\")",
            "return": 1609459200000000
          },
          {
            "title": "Convert to a Unix timestamp (nanoseconds)",
            "source": "to_unix_timestamp(t'2021-01-01T00:00:00Z', unit: \"nanoseconds\")",
            "return": 1609459200000000000
          }
        ],
        "pure": true
      }
    }
  }
}
