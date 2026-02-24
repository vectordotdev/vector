{
  "remap": {
    "functions": {
      "bool": {
        "anchor": "bool",
        "name": "bool",
        "category": "Type",
        "description": "Returns `value` if it is a Boolean, otherwise returns an error. This enables the type\nchecker to guarantee that the returned value is a Boolean and can be used in any\nfunction that expects a Boolean.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check if it is a Boolean.",
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
            "Returns `value` if it's a Boolean.",
            "Raises an error if not a Boolean."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a Boolean."
        ],
        "examples": [
          {
            "title": "Valid Boolean",
            "source": "bool(false)",
            "return": false
          },
          {
            "title": "Invalid Boolean",
            "source": "bool!(42)",
            "raises": "function call error for \"bool\" at (0:9): expected boolean, got integer"
          },
          {
            "title": "Valid Boolean from path",
            "source": "bool!(.value)",
            "input": "{ \"value\": true }",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}
