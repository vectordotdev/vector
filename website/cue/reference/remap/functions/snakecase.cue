{
  "remap": {
    "functions": {
      "snakecase": {
        "anchor": "snakecase",
        "name": "snakecase",
        "category": "String",
        "description": "Takes the `value` string, and turns it into snake_case. Optionally, you can pass in the existing case of the function, or else we will try to figure out the case automatically.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to convert to snake_case.",
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
          },
          {
            "name": "excluded_boundaries",
            "description": "Case boundaries to exclude during conversion.",
            "required": false,
            "type": [
              "array"
            ],
            "enum": {
              "lower_upper": "Lowercase to uppercase transitions (e.g., 'camelCase' → 'camel' + 'case')",
              "upper_lower": "Uppercase to lowercase transitions (e.g., 'CamelCase' → 'Camel' + 'Case')",
              "acronym": "Acronyms from words (e.g., 'XMLHttpRequest' → 'xmlhttp' + 'request')",
              "lower_digit": "Lowercase to digit transitions (e.g., 'foo2bar' → 'foo2_bar')",
              "upper_digit": "Uppercase to digit transitions (e.g., 'versionV2' → 'version_v2')",
              "digit_lower": "Digit to lowercase transitions (e.g., 'Foo123barBaz' → 'foo' + '123bar' + 'baz')",
              "digit_upper": "Digit to uppercase transitions (e.g., 'Version123Test' → 'version' + '123test')"
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
            "title": "snake_case a string",
            "source": "snakecase(\"input-string\")",
            "return": "input_string"
          },
          {
            "title": "snake_case a string with original case",
            "source": "snakecase(\"input-string\", original_case: \"kebab-case\")",
            "return": "input_string"
          },
          {
            "title": "snake_case with excluded boundaries",
            "source": "snakecase(\"s3BucketDetails\", excluded_boundaries: [\"lower_digit\"])",
            "return": "s3_bucket_details"
          }
        ],
        "pure": true
      }
    }
  }
}
