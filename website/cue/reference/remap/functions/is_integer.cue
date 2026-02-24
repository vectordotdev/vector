{
  "remap": {
    "functions": {
      "is_integer": {
        "anchor": "is_integer",
        "name": "is_integer",
        "category": "Type",
        "description": "Check if the `value`'s type is an integer.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is an integer.",
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
            "Returns `true` if `value` is an integer.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid integer",
            "source": "is_integer(1)",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_integer(\"a string\")",
            "return": false
          },
          {
            "title": "Null",
            "source": "is_integer(null)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
