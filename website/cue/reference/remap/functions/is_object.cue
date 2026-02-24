{
  "remap": {
    "functions": {
      "is_object": {
        "anchor": "is_object",
        "name": "is_object",
        "category": "Type",
        "description": "Check if `value`'s type is an object.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is an object.",
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
            "Returns `true` if `value` is an object.",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Valid object",
            "source": "is_object({\"foo\": \"bar\"})",
            "return": true
          },
          {
            "title": "Non-matching type",
            "source": "is_object(\"a string\")",
            "return": false
          },
          {
            "title": "Boolean",
            "source": "is_object(true)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
