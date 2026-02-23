{
  "remap": {
    "functions": {
      "object": {
        "anchor": "object",
        "name": "object",
        "category": "Type",
        "description": "Returns `value` if it is an object, otherwise returns an error. This enables the type checker to guarantee that the returned value is an object and can be used in any function that expects an object.",
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
            "object"
          ],
          "rules": [
            "Returns the `value` if it's an object.",
            "Raises an error if not an object."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not an object."
        ],
        "examples": [
          {
            "title": "Declare an object type",
            "source": ". = { \"value\": { \"field1\": \"value1\", \"field2\": \"value2\" } }\nobject(.value)\n",
            "return": {
              "field1": "value1",
              "field2": "value2"
            }
          },
          {
            "title": "Invalid type",
            "source": "object!(true)",
            "raises": "function call error for \"object\" at (0:13): expected object, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}