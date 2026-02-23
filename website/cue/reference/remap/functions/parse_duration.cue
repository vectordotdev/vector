{
  "remap": {
    "functions": {
      "parse_duration": {
        "anchor": "parse_duration",
        "name": "parse_duration",
        "category": "Parse",
        "description": "Parses the `value` into a human-readable duration format specified by `unit`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string of the duration.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "unit",
            "description": "The output units for the duration.",
            "required": true,
            "type": [
              "string"
            ],
            "enum": {
              "ms": "Milliseconds (1 thousand microseconds in a second)",
              "ds": "Deciseconds (10 deciseconds in a second)",
              "d": "Days (24 hours in a day)",
              "m": "Minutes (60 seconds in a minute)",
              "h": "Hours (60 minutes in an hour)",
              "cs": "Centiseconds (100 centiseconds in a second)",
              "s": "Seconds",
              "ns": "Nanoseconds (1 billion nanoseconds in a second)",
              "us": "Microseconds (1 million microseconds in a second)",
              "µs": "Microseconds (1 million microseconds in a second)"
            }
          }
        ],
        "return": {
          "types": [
            "float"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted duration."
        ],
        "examples": [
          {
            "title": "Parse duration (milliseconds)",
            "source": "parse_duration!(\"1005ms\", unit: \"s\")",
            "return": 1.005
          },
          {
            "title": "Parse multiple durations (seconds & milliseconds)",
            "source": "parse_duration!(\"1s 1ms\", unit: \"ms\")",
            "return": 1001.0
          }
        ],
        "pure": true
      }
    }
  }
}