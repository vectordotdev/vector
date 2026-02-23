{
  "remap": {
    "functions": {
      "parse_grok": {
        "anchor": "parse_grok",
        "name": "parse_grok",
        "category": "Parse",
        "description": "Parses the `value` using the [`grok`](https://github.com/daschl/grok/tree/master/patterns) format. All patterns [listed here](https://github.com/daschl/grok/tree/master/patterns) are supported.",
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
            "name": "pattern",
            "description": "The [Grok pattern](https://github.com/daschl/grok/tree/master/patterns).",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` fails to parse using the provided `pattern`."
        ],
        "examples": [
          {
            "title": "Parse using Grok",
            "source": "value = \"2020-10-02T23:22:12.223222Z info Hello world\"\npattern = \"%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}\"\n\nparse_grok!(value, pattern)\n",
            "return": {
              "level": "info",
              "message": "Hello world",
              "timestamp": "2020-10-02T23:22:12.223222Z"
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