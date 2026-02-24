{
  "remap": {
    "functions": {
      "sha3": {
        "anchor": "sha3",
        "name": "sha3",
        "category": "Cryptography",
        "description": "Calculates a [SHA-3](https://en.wikipedia.org/wiki/SHA-3) hash of the `value`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to calculate the hash for.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "variant",
            "description": "The variant of the algorithm to use.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "SHA3-224": "SHA3-224 algorithm",
              "SHA3-256": "SHA3-256 algorithm",
              "SHA3-384": "SHA3-384 algorithm",
              "SHA3-512": "SHA3-512 algorithm"
            },
            "default": "SHA3-512"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Calculate sha3 hash using default variant",
            "source": "sha3(\"foobar\")",
            "return": "ff32a30c3af5012ea395827a3e99a13073c3a8d8410a708568ff7e6eb85968fccfebaea039bc21411e9d43fdb9a851b529b9960ffea8679199781b8f45ca85e2"
          },
          {
            "title": "Calculate sha3 hash with SHA3-224",
            "source": "sha3(\"foo\", variant: \"SHA3-224\")",
            "return": "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a"
          },
          {
            "title": "Calculate sha3 hash with SHA3-384",
            "source": "sha3(\"foobar\", \"SHA3-384\")",
            "return": "0fa8abfbdaf924ad307b74dd2ed183b9a4a398891a2f6bac8fd2db7041b77f068580f9c6c66f699b496c2da1cbcc7ed8"
          }
        ],
        "pure": true
      }
    }
  }
}
