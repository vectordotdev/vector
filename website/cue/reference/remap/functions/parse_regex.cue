{
  "remap": {
    "functions": {
      "parse_regex": {
        "anchor": "parse_regex",
        "name": "parse_regex",
        "category": "Parse",
        "description": "Parses the `value` using the provided [Regex](https://en.wikipedia.org/wiki/Regular_expression) `pattern`.\n\nThis function differs from the `parse_regex_all` function in that it returns only the first match.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to search.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "pattern",
            "description": "The regular expression pattern to search against.",
            "required": true,
            "type": [
              "regex"
            ]
          },
          {
            "name": "numeric_groups",
            "description": "If true, the index of each group in the regular expression is also captured. Index `0`\ncontains the whole match.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "object"
          ],
          "rules": [
            "Matches return all capture groups corresponding to the leftmost matches in the text.",
            "Raises an error if no match is found."
          ]
        },
        "internal_failure_reasons": [
          "`value` fails to parse using the provided `pattern`."
        ],
        "examples": [
          {
            "title": "Parse using Regex (with capture groups)",
            "source": "parse_regex!(\"first group and second group.\", r'(?P<number>.*?) group')",
            "return": {
              "number": "first"
            }
          },
          {
            "title": "Parse using Regex (without capture groups)",
            "source": "parse_regex!(\"first group and second group.\", r'(\\w+) group', numeric_groups: true)",
            "return": {
              "0": "first group",
              "1": "first"
            }
          },
          {
            "title": "Parse using Regex with simple match",
            "source": "parse_regex!(\"8.7.6.5 - zorp\", r'^(?P<host>[\\w\\.]+) - (?P<user>[\\w]+)')",
            "return": {
              "host": "8.7.6.5",
              "user": "zorp"
            }
          },
          {
            "title": "Parse using Regex with all numeric groups",
            "source": "parse_regex!(\"8.7.6.5 - zorp\", r'^(?P<host>[\\w\\.]+) - (?P<user>[\\w]+)', numeric_groups: true)",
            "return": {
              "0": "8.7.6.5 - zorp",
              "1": "8.7.6.5",
              "2": "zorp",
              "host": "8.7.6.5",
              "user": "zorp"
            }
          },
          {
            "title": "Parse using Regex with variables",
            "source": "variable = r'^(?P<host>[\\w\\.]+) - (?P<user>[\\w]+)';\nparse_regex!(\"8.7.6.5 - zorp\", variable)\n",
            "return": {
              "host": "8.7.6.5",
              "user": "zorp"
            }
          }
        ],
        "notices": [
          "VRL aims to provide purpose-specific [parsing functions](/docs/reference/vrl/functions/#parse-functions)\nfor common log formats. Before reaching for the `parse_regex` function, see if a VRL\n[`parse_*` function](/docs/reference/vrl/functions/#parse-functions) already exists\nfor your format. If not, we recommend\n[opening an issue](https://github.com/vectordotdev/vector/issues/new?labels=type%3A+new+feature)\nto request support for the desired format.",
          "All values are returned as strings. We recommend manually coercing values to desired\ntypes as you see fit."
        ],
        "pure": true
      }
    }
  }
}