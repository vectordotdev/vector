{
  "remap": {
    "functions": {
      "is_string": {
        "anchor": "is_string",
        "name": "is_string",
        "category": "Type",
        "description": "Check if `value`'s type is a string.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a string.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ],
          "rules": [
            "Returns `true` if `value` is a string.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid string",
            "source": "is_string(\"a string\")",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_string([1, 2, 3])",
            "return": false
          },
          {
            "title": "Boolean",
            "source": "is_string(true)",
            "return": false
          },
          {
            "title": "Null",
            "source": "is_string(null)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
