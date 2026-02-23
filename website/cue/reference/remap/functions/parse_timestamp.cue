{
  "remap": {
    "functions": {
      "parse_timestamp": {
        "anchor": "parse_timestamp",
        "name": "parse_timestamp",
        "category": "Parse",
        "description": "Parses the `value` in [strptime](https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers) `format`.",
        "arguments": [
          {
            "name": "value",
            "description": "The text of the timestamp.",
            "required": true,
            "type": [
              "string",
              "timestamp"
            ]
          },
          {
            "name": "format",
            "description": "The [strptime](https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers) format.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "timezone",
            "description": "The [TZ database](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones) format. By default, this function parses the timestamp by global [`timezone` option](/docs/reference/configuration//global-options#timezone).\nThis argument overwrites the setting and is useful for parsing timestamps without a specified timezone, such as `16/10/2019 12:00:00`.",
            "required": false,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "timestamp"
          ]
        },
        "internal_failure_reasons": [
          "`value` fails to parse using the provided `format`.",
          "`value` fails to parse using the provided `timezone`."
        ],
        "examples": [
          {
            "title": "Parse timestamp",
            "source": "parse_timestamp!(\"10-Oct-2020 16:00+00:00\", format: \"%v %R %:z\")",
            "return": "t'2020-10-10T16:00:00Z'"
          },
          {
            "title": "Parse timestamp with timezone",
            "source": "parse_timestamp!(\"16/10/2019 12:00:00\", format: \"%d/%m/%Y %H:%M:%S\", timezone: \"Asia/Taipei\")",
            "return": "t'2019-10-16T04:00:00Z'"
          }
        ],
        "pure": true
      }
    }
  }
}