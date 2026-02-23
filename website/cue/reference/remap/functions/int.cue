{
  "remap": {
    "functions": {
      "int": {
        "anchor": "int",
        "name": "int",
        "category": "Type",
        "description": "Returns `value` if it is an integer, otherwise returns an error. This enables the type checker to guarantee that the returned value is an integer and can be used in any function that expects an integer.",
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
            "integer"
          ],
          "rules": [
            "Returns the `value` if it's an integer.",
            "Raises an error if not an integer."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not an integer."
        ],
        "examples": [
          {
            "title": "Declare an integer type",
            "source": ". = { \"value\": 42 }\nint(.value)\n",
            "return": 42
          },
          {
            "title": "Declare an integer type (literal)",
            "source": "int(42)",
            "return": 42
          },
          {
            "title": "Invalid integer type",
            "source": "int!(true)",
            "raises": "function call error for \"int\" at (0:10): expected integer, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}