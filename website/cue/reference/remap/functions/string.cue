{
  "remap": {
    "functions": {
      "string": {
        "anchor": "string",
        "name": "string",
        "category": "Type",
        "description": "Returns `value` if it is a string, otherwise returns an error. This enables the type checker to guarantee that the returned value is a string and can be used in any function that expects a string.",
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
            "string"
          ],
          "rules": [
            "Returns the `value` if it's a string.",
            "Raises an error if not a string."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a string."
        ],
        "examples": [
          {
            "title": "Declare a string type",
            "source": ". = { \"message\": \"{\\\"field\\\": \\\"value\\\"}\" }\nstring(.message)\n",
            "return": "{\"field\": \"value\"}"
          },
          {
            "title": "Invalid type",
            "source": "string!(true)",
            "raises": "function call error for \"string\" at (0:13): expected string, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}
