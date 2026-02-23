{
  "remap": {
    "functions": {
      "is_float": {
        "anchor": "is_float",
        "name": "is_float",
        "category": "Type",
        "description": "Check if the `value`'s type is a float.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a float.",
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
            "Returns `true` if `value` is a float.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid float",
            "source": "is_float(0.577)",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_float(\"a string\")",
            "return": false
          },
          {
            "title": "Boolean",
            "source": "is_float(true)",
            "return": false
          },
          {
            "title": "Null",
            "source": "is_float(null)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}