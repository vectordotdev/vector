{
  "remap": {
    "functions": {
      "is_array": {
        "anchor": "is_array",
        "name": "is_array",
        "category": "Type",
        "description": "Check if the `value`'s type is an array.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is an array.",
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
            "Returns `true` if `value` is an array.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid array",
            "source": "is_array([1, 2, 3])",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_array(\"a string\")",
            "return": false
          },
          {
            "title": "Boolean",
            "source": "is_array(true)",
            "return": false
          },
          {
            "title": "Null",
            "source": "is_array(null)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}