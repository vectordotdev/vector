{
  "remap": {
    "functions": {
      "pascalcase": {
        "anchor": "pascalcase",
        "name": "pascalcase",
        "category": "String",
        "description": "Takes the `value` string, and turns it into PascalCase. Optionally, you can pass in the existing case of the function, or else we will try to figure out the case automatically.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to PascalCase.",
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
              "kebab-case": "[kebab-case](https://en.wikipedia.org/wiki/Letter_case#Kebab_case)",
              "camelCase": "[camelCase](https://en.wikipedia.org/wiki/Camel_case)",
              "PascalCase": "[PascalCase](https://en.wikipedia.org/wiki/Camel_case)",
              "SCREAMING_SNAKE": "[SCREAMING_SNAKE](https://en.wikipedia.org/wiki/Snake_case)",
              "snake_case": "[snake_case](https://en.wikipedia.org/wiki/Snake_case)"
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
            "title": "PascalCase a string without specifying original case",
            "source": "pascalcase(\"input-string\")",
            "return": "InputString"
          },
          {
            "title": "PascalCase a snake_case string",
            "source": "pascalcase(\"foo_bar_baz\", \"snake_case\")",
            "return": "FooBarBaz"
          },
          {
            "title": "PascalCase specifying the wrong original case (only capitalizes)",
            "source": "pascalcase(\"foo_bar_baz\", \"kebab-case\")",
            "return": "Foo_bar_baz"
          }
        ],
        "pure": true
      }
    }
  }
}
