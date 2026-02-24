{
  "remap": {
    "functions": {
      "camelcase": {
        "anchor": "camelcase",
        "name": "camelcase",
        "category": "String",
        "description": "Takes the `value` string, and turns it into camelCase. Optionally, you can pass in the existing case of the function, or else an attempt is made to determine the case automatically.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to camelCase.",
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
            "title": "camelCase a string without specifying original case",
            "source": "camelcase(\"input-string\")",
            "return": "inputString"
          },
          {
            "title": "camelcase a snake_case string",
            "source": "camelcase(\"foo_bar_baz\", \"snake_case\")",
            "return": "fooBarBaz"
          },
          {
            "title": "camelcase specifying the wrong original case (noop)",
            "source": "camelcase(\"foo_bar_baz\", \"kebab-case\")",
            "return": "foo_bar_baz"
          }
        ],
        "pure": true
      }
    }
  }
}
