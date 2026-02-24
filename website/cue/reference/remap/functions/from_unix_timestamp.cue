{
  "remap": {
    "functions": {
      "from_unix_timestamp": {
        "anchor": "from_unix_timestamp",
        "name": "from_unix_timestamp",
        "category": "Convert",
        "description": "Converts the `value` integer from a [Unix timestamp](https://en.wikipedia.org/wiki/Unix_time) to a VRL `timestamp`.\n\nConverts from the number of seconds since the Unix epoch by default. To convert from milliseconds or nanoseconds, set the `unit` argument to `milliseconds` or `nanoseconds`.",
        "arguments": [
          {
            "name": "value",
            "description": "The Unix timestamp to convert.",
            "required": true,
            "type": [
              "integer"
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
              "milliseconds": "Express Unix time in milliseconds",
              "nanoseconds": "Express Unix time in nanoseconds",
              "microseconds": "Express Unix time in microseconds"
            },
            "default": "seconds"
          }
        ],
        "return": {
          "types": [
            "timestamp"
          ]
        },
        "examples": [
          {
            "title": "Convert from a Unix timestamp (seconds)",
            "source": "from_unix_timestamp!(5)",
            "return": "t'1970-01-01T00:00:05Z'"
          },
          {
            "title": "Convert from a Unix timestamp (milliseconds)",
            "source": "from_unix_timestamp!(5000, unit: \"milliseconds\")",
            "return": "t'1970-01-01T00:00:05Z'"
          },
          {
            "title": "Convert from a Unix timestamp (microseconds)",
            "source": "from_unix_timestamp!(5000, unit: \"microseconds\")",
            "return": "t'1970-01-01T00:00:00.005Z'"
          },
          {
            "title": "Convert from a Unix timestamp (nanoseconds)",
            "source": "from_unix_timestamp!(5000, unit: \"nanoseconds\")",
            "return": "t'1970-01-01T00:00:00.000005Z'"
          }
        ],
        "pure": true
      }
    }
  }
}
