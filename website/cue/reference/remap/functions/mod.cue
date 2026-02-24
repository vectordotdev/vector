{
  "remap": {
    "functions": {
      "mod": {
        "anchor": "mod",
        "name": "mod",
        "category": "Number",
        "description": "Calculates the remainder of `value` divided by `modulus`.",
        "arguments": [
          {
            "name": "value",
            "description": "The value the `modulus` is applied to.",
            "required": true,
            "type": [
              "integer",
              "float"
            ]
          },
          {
            "name": "modulus",
            "description": "The `modulus` value.",
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
          ]
        },
        "internal_failure_reasons": [
          "`value` is not an integer or float.",
          "`modulus` is not an integer or float.",
          "`modulus` is equal to 0."
        ],
        "examples": [
          {
            "title": "Calculate the remainder of two integers",
            "source": "mod(5, 2)",
            "return": 1
          }
        ],
        "pure": true
      }
    }
  }
}
