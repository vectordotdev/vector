{
  "remap": {
    "functions": {
      "sha1": {
        "anchor": "sha1",
        "name": "sha1",
        "category": "Cryptography",
        "description": "Calculates a [SHA-1](https://en.wikipedia.org/wiki/SHA-1) hash of the `value`.",
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
            "string"
          ]
        },
        "examples": [
          {
            "title": "Calculate sha1 hash",
            "source": "sha1(\"foo\")",
            "return": "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33"
          }
        ],
        "pure": true
      }
    }
  }
}