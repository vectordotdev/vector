{
  "remap": {
    "functions": {
      "is_null": {
        "anchor": "is_null",
        "name": "is_null",
        "category": "Type",
        "description": "Check if `value`'s type is `null`. For a more relaxed function, see [`is_nullish`](/docs/reference/vrl/functions#is_nullish).",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is `null`.",
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
            "Returns `true` if `value` is null.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Null value",
            "source": "is_null(null)",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_null(\"a string\")",
            "return": false
          },
          {
            "title": "Array",
            "source": "is_null([1, 2, 3])",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
