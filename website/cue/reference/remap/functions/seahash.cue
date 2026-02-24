{
  "remap": {
    "functions": {
      "seahash": {
        "anchor": "seahash",
        "name": "seahash",
        "category": "Cryptography",
        "description": "Calculates a [Seahash](https://docs.rs/seahash/latest/seahash/) hash of the `value`.\n**Note**: Due to limitations in the underlying VRL data types, this function converts the unsigned 64-bit integer SeaHash result to a signed 64-bit integer. Results higher than the signed 64-bit integer maximum value wrap around to negative values.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to calculate the hash for.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "examples": [
          {
            "title": "Calculate seahash",
            "source": "seahash(\"foobar\")",
            "return": 5348458858952426560
          },
          {
            "title": "Calculate negative seahash",
            "source": "seahash(\"bar\")",
            "return": -2796170501982571315
          }
        ],
        "pure": true
      }
    }
  }
}
