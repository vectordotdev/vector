{
  "remap": {
    "functions": {
      "parse_groks": {
        "anchor": "parse_groks",
        "name": "parse_groks",
        "category": "Parse",
        "description": "Parses the `value` using multiple [`grok`](https://github.com/daschl/grok/tree/master/patterns) patterns. All patterns [listed here](https://github.com/daschl/grok/tree/master/patterns) are supported.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "patterns",
            "description": "The [Grok patterns](https://github.com/daschl/grok/tree/master/patterns), which are tried in order until the first match.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "aliases",
            "description": "The shared set of grok aliases that can be referenced in the patterns to simplify them.",
            "required": false,
            "type": [
              "object"
            ],
            "default": "{  }"
          },
          {
            "name": "alias_sources",
            "description": "Path to the file containing aliases in a JSON format.",
            "required": false,
            "type": [
              "array"
            ],
            "default": "[]"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` fails to parse using the provided `pattern`.",
          "`patterns` is not an array.",
          "`aliases` is not an object.",
          "`alias_sources` is not a string array or doesn't point to a valid file."
        ],
        "examples": [
          {
            "title": "Parse using multiple Grok patterns",
            "source": "parse_groks!(\n    \"2020-10-02T23:22:12.223222Z info Hello world\",\n    patterns: [\n        \"%{common_prefix} %{_status} %{_message}\",\n        \"%{common_prefix} %{_message}\",\n    ],\n    aliases: {\n        \"common_prefix\": \"%{_timestamp} %{_loglevel}\",\n        \"_timestamp\": \"%{TIMESTAMP_ISO8601:timestamp}\",\n        \"_loglevel\": \"%{LOGLEVEL:level}\",\n        \"_status\": \"%{POSINT:status}\",\n        \"_message\": \"%{GREEDYDATA:message}\"\n    }\n)\n",
            "return": {
              "level": "info",
              "message": "Hello world",
              "timestamp": "2020-10-02T23:22:12.223222Z"
            }
          },
          {
            "title": "Parse using aliases from file",
            "source": "parse_groks!(\n  \"username=foo\",\n  patterns: [ \"%{PATTERN_A}\" ],\n  alias_sources: [ \"tests/data/grok/aliases.json\" ]\n)\n# aliases.json contents:\n# {\n#   \"PATTERN_A\": \"%{PATTERN_B}\",\n#   \"PATTERN_B\": \"username=%{USERNAME:username}\"\n# }\n",
            "return": {
              "username": "foo"
            }
          }
        ],
        "notices": [
          "We recommend using community-maintained Grok patterns when possible, as they're more\nlikely to be properly vetted and improved over time than bespoke patterns."
        ],
        "pure": true
      }
    }
  }
}