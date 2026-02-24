{
  "remap": {
    "functions": {
      "random_bytes": {
        "anchor": "random_bytes",
        "name": "random_bytes",
        "category": "Random",
        "description": "A cryptographically secure random number generator. Returns a string value containing the number of random bytes requested.",
        "arguments": [
          {
            "name": "length",
            "description": "The number of bytes to generate. Must not be larger than 64k.",
            "required": true,
            "type": [
              "integer"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`length` is negative.",
          "`length` is larger than the maximum value (64k)."
        ],
        "examples": [
          {
            "title": "Generate random base 64 encoded bytes",
            "source": "encode_base64(random_bytes(16))",
            "return": "LNu0BBgUbh7XAlXbjSOomQ=="
          },
          {
            "title": "Generate 16 random bytes",
            "source": "length(random_bytes(16))",
            "return": 16
          }
        ],
        "pure": true
      }
    }
  }
}
