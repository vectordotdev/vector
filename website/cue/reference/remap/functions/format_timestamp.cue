{
  "remap": {
    "functions": {
      "format_timestamp": {
        "anchor": "format_timestamp",
        "name": "format_timestamp",
        "category": "Timestamp",
        "description": "Formats `value` into a string representation of the timestamp.",
        "arguments": [
          {
            "name": "value",
            "description": "The timestamp to format as text.",
            "required": true,
            "type": [
              "timestamp"
            ]
          },
          {
            "name": "format",
            "description": "The format string as described by the [Chrono library](https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers).",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "timezone",
            "description": "The timezone to use when formatting the timestamp. The parameter uses the TZ identifier or `local`.",
            "required": false,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Format a timestamp (ISO8601/RFC 3339)",
            "source": "format_timestamp!(t'2020-10-21T16:00:00Z', format: \"%+\")",
            "return": "2020-10-21T16:00:00+00:00"
          },
          {
            "title": "Format a timestamp (custom)",
            "source": "format_timestamp!(t'2020-10-21T16:00:00Z', format: \"%v %R\")",
            "return": "21-Oct-2020 16:00"
          },
          {
            "title": "Format a timestamp with custom format string",
            "source": "format_timestamp!(t'2021-02-10T23:32:00+00:00', format: \"%d %B %Y %H:%M\")",
            "return": "10 February 2021 23:32"
          },
          {
            "title": "Format a timestamp with timezone conversion",
            "source": "format_timestamp!(t'2021-02-10T23:32:00+00:00', format: \"%d %B %Y %H:%M\", timezone: \"Europe/Berlin\")",
            "return": "11 February 2021 00:32"
          }
        ],
        "pure": true
      }
    }
  }
}
