{
  "remap": {
    "functions": {
      "abs": {
        "anchor": "abs",
        "name": "abs",
        "category": "Number",
        "description": "Computes the absolute value of `value`.",
        "arguments": [
          {
            "name": "value",
            "description": "The number to calculate the absolute value.",
            "required": true,
            "type": [
              "integer",
              "float"
            ]
          }
        ],
        "return": {
          "types": [
            "integer",
            "float"
          ],
          "rules": [
            "Returns the absolute value."
          ]
        },
        "examples": [
          {
            "title": "Computes the absolute value of an integer",
            "source": "abs(-42)",
            "return": 42
          },
          {
            "title": "Computes the absolute value of a float",
            "source": "abs(-42.2)",
            "return": 42.2
          },
          {
            "title": "Computes the absolute value of a positive integer",
            "source": "abs(10)",
            "return": 10
          }
        ],
        "pure": true
      }
    }
  }
}