{
  "remap": {
    "functions": {
      "kebabcase": {
        "anchor": "kebabcase",
        "name": "kebabcase",
        "category": "String",
        "description": "Takes the `value` string, and turns it into kebab-case. Optionally, you can pass in the existing case of the function, or else we will try to figure out the case automatically.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to kebab-case.",
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
              "camelCase": "[camelCase](https://en.wikipedia.org/wiki/Camel_case)",
              "snake_case": "[snake_case](https://en.wikipedia.org/wiki/Snake_case)",
              "SCREAMING_SNAKE": "[SCREAMING_SNAKE](https://en.wikipedia.org/wiki/Snake_case)"
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
            "title": "kebab-case a string without specifying original case",
            "source": "kebabcase(\"InputString\")",
            "return": "input-string"
          },
          {
            "title": "kebab-case a snake_case string",
            "source": "kebabcase(\"foo_bar_baz\", \"snake_case\")",
            "return": "foo-bar-baz"
          },
          {
            "title": "kebab-case specifying the wrong original case (noop)",
            "source": "kebabcase(\"foo_bar_baz\", \"PascalCase\")",
            "return": "foo_bar_baz"
          }
        ],
        "pure": true
      }
    }
  }
}
