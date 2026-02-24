{
  "remap": {
    "functions": {
      "is_empty": {
        "anchor": "is_empty",
        "name": "is_empty",
        "category": "Type",
        "description": "Check if the object, array, or string has a length of `0`.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check.",
            "required": true,
            "type": [
              "string",
              "object",
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ],
          "rules": [
            "Returns `true` if `value` is empty.",
            "Returns `false` if `value` is non-empty."
          ]
        },
        "examples": [
          {
            "title": "Empty array",
            "source": "is_empty([])",
            "return": true
          },
          {
            "title": "Non-empty string",
            "source": "is_empty(\"a string\")",
            "return": false
          },
          {
            "title": "Non-empty object",
            "source": "is_empty({\"foo\": \"bar\"})",
            "return": false
          },
          {
            "title": "Empty string",
            "source": "is_empty(\"\")",
            "return": true
          },
          {
            "title": "Empty object",
            "source": "is_empty({})",
            "return": true
          },
          {
            "title": "Non-empty array",
            "source": "is_empty([1,2,3])",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
