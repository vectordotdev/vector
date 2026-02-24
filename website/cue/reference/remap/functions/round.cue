{
  "remap": {
    "functions": {
      "round": {
        "anchor": "round",
        "name": "round",
        "category": "Number",
        "description": "Rounds the `value` to the specified `precision`.",
        "arguments": [
          {
            "name": "value",
            "description": "The number to round.",
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
            "If `precision` is `0`, then an integer is returned, otherwise a float is returned."
          ]
        },
        "examples": [
          {
            "title": "Round a number (without precision)",
            "source": "round(4.345)",
            "return": 4.0
          },
          {
            "title": "Round a number (with precision)",
            "source": "round(4.345, precision: 2)",
            "return": 4.35
          },
          {
            "title": "Round up",
            "source": "round(5.5)",
            "return": 6.0
          },
          {
            "title": "Round down",
            "source": "round(5.45)",
            "return": 5.0
          }
        ],
        "pure": true
      }
    }
  }
}
