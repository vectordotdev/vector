{
  "remap": {
    "functions": {
      "floor": {
        "anchor": "floor",
        "name": "floor",
        "category": "Number",
        "description": "Rounds the `value` down to the specified `precision`.",
        "arguments": [
          {
            "name": "value",
            "description": "The number to round down.",
            "required": true,
            "type": [
              "integer",
              "float"
            ]
          },
          {
            "name": "precision",
            "description": "The number of decimal places to round to.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "0"
          }
        ],
        "return": {
          "types": [
            "integer",
            "float"
          ],
          "rules": [
            "Returns an integer if `precision` is `0` (this is the default). Returns a float otherwise."
          ]
        },
        "examples": [
          {
            "title": "Round a number down (without precision)",
            "source": "floor(9.8)",
            "return": 9.0
          },
          {
            "title": "Round a number down (with precision)",
            "source": "floor(4.345, precision: 2)",
            "return": 4.34
          }
        ],
        "pure": true
      }
    }
  }
}