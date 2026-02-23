{
  "remap": {
    "functions": {
      "float": {
        "anchor": "float",
        "name": "float",
        "category": "Type",
        "description": "Returns `value` if it is a float, otherwise returns an error. This enables the type checker to guarantee that the returned value is a float and can be used in any function that expects a float.",
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
            "float"
          ],
          "rules": [
            "Returns the `value` if it's a float.",
            "Raises an error if not a float."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a float."
        ],
        "examples": [
          {
            "title": "Declare a float type",
            "source": ". = { \"value\": 42.0 }\nfloat(.value)\n",
            "return": 42.0
          },
          {
            "title": "Declare a float type (literal)",
            "source": "float(3.1415)",
            "return": 3.1415
          },
          {
            "title": "Invalid float type",
            "source": "float!(true)",
            "raises": "function call error for \"float\" at (0:12): expected float, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}