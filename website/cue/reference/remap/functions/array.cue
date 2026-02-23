{
  "remap": {
    "functions": {
      "array": {
        "anchor": "array",
        "name": "array",
        "category": "Type",
        "description": "Returns `value` if it is an array, otherwise returns an error. This enables the type checker to guarantee that the returned value is an array and can be used in any function that expects an array.",
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
            "array"
          ],
          "rules": [
            "Returns the `value` if it's an array.",
            "Raises an error if not an array."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not an array."
        ],
        "examples": [
          {
            "title": "Declare an array type",
            "source": ".value = [1, 2, 3]\narray(.value)\n",
            "return": [
              1,
              2,
              3
            ]
          },
          {
            "title": "Valid array literal",
            "source": "array([1,2,3])",
            "return": [
              1,
              2,
              3
            ]
          },
          {
            "title": "Invalid type",
            "source": "array!(true)",
            "raises": "function call error for \"array\" at (0:12): expected array, got boolean"
          }
        ],
        "pure": true
      }
    }
  }
}