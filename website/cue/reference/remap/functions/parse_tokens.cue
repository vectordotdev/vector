{
  "remap": {
    "functions": {
      "parse_tokens": {
        "anchor": "parse_tokens",
        "name": "parse_tokens",
        "category": "Parse",
        "description": "Parses the `value` in token format. A token is considered to be one of the following:\n\n* A word surrounded by whitespace.\n* Text delimited by double quotes: `\"..\"`. Quotes can be included in the token if they are escaped by a backslash (`\\`).\n* Text delimited by square brackets: `[..]`. Closing square brackets can be included in the token if they are escaped by a backslash (`\\`).",
        "arguments": [
          {
            "name": "value",
            "description": "The string to tokenize.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a properly formatted tokenized string."
        ],
        "examples": [
          {
            "title": "Parse tokens",
            "source": "parse_tokens(s'A sentence \"with \\\"a\\\" sentence inside\" and [some brackets]')",
            "return": [
              "A",
              "sentence",
              "with \\\"a\\\" sentence inside",
              "and",
              "some brackets"
            ]
          }
        ],
        "notices": [
          "All token values are returned as strings. We recommend manually coercing values to\ndesired types as you see fit."
        ],
        "pure": true
      }
    }
  }
}
