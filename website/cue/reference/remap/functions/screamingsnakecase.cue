{
  "remap": {
    "functions": {
      "screamingsnakecase": {
        "anchor": "screamingsnakecase",
        "name": "screamingsnakecase",
        "category": "String",
        "description": "Takes the `value` string, and turns it into SCREAMING_SNAKE case. Optionally, you can pass in the existing case of the function, or else we will try to figure out the case automatically.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to SCREAMING_SNAKE case.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "original_case",
            "description": "Optional hint on the original case type. Must be one of: kebab-case, camelCase, PascalCase, SCREAMING_SNAKE, snake_case",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "PascalCase": "[PascalCase](https://en.wikipedia.org/wiki/Camel_case)",
              "kebab-case": "[kebab-case](https://en.wikipedia.org/wiki/Letter_case#Kebab_case)",
              "SCREAMING_SNAKE": "[SCREAMING_SNAKE](https://en.wikipedia.org/wiki/Snake_case)",
              "snake_case": "[snake_case](https://en.wikipedia.org/wiki/Snake_case)",
              "camelCase": "[camelCase](https://en.wikipedia.org/wiki/Camel_case)"
            }
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "SCREAMING_SNAKE_CASE a string without specifying original case",
            "source": "screamingsnakecase(\"input-string\")",
            "return": "INPUT_STRING"
          },
          {
            "title": "SCREAMING_SNAKE_CASE a snake_case string",
            "source": "screamingsnakecase(\"foo_bar_baz\", \"snake_case\")",
            "return": "FOO_BAR_BAZ"
          },
          {
            "title": "SCREAMING_SNAKE_CASE specifying the wrong original case (capitalizes but doesn't include `_` properly)",
            "source": "screamingsnakecase(\"FooBarBaz\", \"kebab-case\")",
            "return": "FOOBARBAZ"
          }
        ],
        "pure": true
      }
    }
  }
}
