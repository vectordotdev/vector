{
  "remap": {
    "functions": {
      "parse_float": {
        "anchor": "parse_float",
        "name": "parse_float",
        "category": "String",
        "description": "Parses the string `value` representing a floating point number in base 10 to a float.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "float"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a string."
        ],
        "examples": [
          {
            "title": "Parse negative integer",
            "source": "parse_float!(\"-42\")",
            "return": -42.0
          },
          {
            "title": "Parse float",
            "source": "parse_float!(\"42.38\")",
            "return": 42.38
          },
          {
            "title": "Scientific notation",
            "source": "parse_float!(\"2.5e3\")",
            "return": 2500.0
          }
        ],
        "pure": true
      }
    }
  }
}