{
  "remap": {
    "functions": {
      "parse_regex_all": {
        "anchor": "parse_regex_all",
        "name": "parse_regex_all",
        "category": "Parse",
        "description": "Parses the `value` using the provided [Regex](https://en.wikipedia.org/wiki/Regular_expression) `pattern`.\n\nThis function differs from the `parse_regex` function in that it returns _all_ matches, not just the first.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to search.",
            "required": true,
            "type": [
              "any"
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
            "description": "If `true`, the index of each group in the regular expression is also captured. Index `0`\ncontains the whole match.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "array"
          ],
          "rules": [
            "Matches return all capture groups corresponding to the leftmost matches in the text.",
            "Raises an error if no match is found."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a string.",
          "`pattern` is not a regex."
        ],
        "examples": [
          {
            "title": "Parse using Regex (all matches)",
            "source": "parse_regex_all!(\"first group and second group.\", r'(?P<number>\\w+) group', numeric_groups: true)",
            "return": [
              {
                "number": "first",
                "0": "first group",
                "1": "first"
              },
              {
                "number": "second",
                "0": "second group",
                "1": "second"
              }
            ]
          },
          {
            "title": "Parse using Regex (simple match)",
            "source": "parse_regex_all!(\"apples and carrots, peaches and peas\", r'(?P<fruit>[\\w\\.]+) and (?P<veg>[\\w]+)')",
            "return": [
              {
                "fruit": "apples",
                "veg": "carrots"
              },
              {
                "fruit": "peaches",
                "veg": "peas"
              }
            ]
          },
          {
            "title": "Parse using Regex (all numeric groups)",
            "source": "parse_regex_all!(\"apples and carrots, peaches and peas\", r'(?P<fruit>[\\w\\.]+) and (?P<veg>[\\w]+)', numeric_groups: true)",
            "return": [
              {
                "fruit": "apples",
                "veg": "carrots",
                "0": "apples and carrots",
                "1": "apples",
                "2": "carrots"
              },
              {
                "fruit": "peaches",
                "veg": "peas",
                "0": "peaches and peas",
                "1": "peaches",
                "2": "peas"
              }
            ]
          },
          {
            "title": "Parse using Regex with variables",
            "source": "variable = r'(?P<fruit>[\\w\\.]+) and (?P<veg>[\\w]+)';\nparse_regex_all!(\"apples and carrots, peaches and peas\", variable)\n",
            "return": [
              {
                "fruit": "apples",
                "veg": "carrots"
              },
              {
                "fruit": "peaches",
                "veg": "peas"
              }
            ]
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
