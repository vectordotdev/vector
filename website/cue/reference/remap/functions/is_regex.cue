{
  "remap": {
    "functions": {
      "is_regex": {
        "anchor": "is_regex",
        "name": "is_regex",
        "category": "Type",
        "description": "Check if `value`'s type is a regex.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a regex.",
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
            "Returns `true` if `value` is a regex.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid regex",
            "source": "is_regex(r'pattern')",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_regex(\"a string\")",
            "return": false
          },
          {
            "title": "Null value",
            "source": "is_regex(null)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}