{
  "remap": {
    "functions": {
      "sha2": {
        "anchor": "sha2",
        "name": "sha2",
        "category": "Cryptography",
        "description": "Calculates a [SHA-2](https://en.wikipedia.org/wiki/SHA-2) hash of the `value`.",
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
              "SHA-512/256": "SHA-512/256 algorithm",
              "SHA-256": "SHA-256 algorithm",
              "SHA-224": "SHA-224 algorithm",
              "SHA-512/224": "SHA-512/224 algorithm",
              "SHA-384": "SHA-384 algorithm",
              "SHA-512": "SHA-512 algorithm"
            },
            "default": "SHA-512/256"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Calculate sha2 hash using default variant",
            "source": "sha2(\"foobar\")",
            "return": "d014c752bc2be868e16330f47e0c316a5967bcbc9c286a457761d7055b9214ce"
          },
          {
            "title": "Calculate sha2 hash with SHA-512/224",
            "source": "sha2(\"foo\", variant: \"SHA-512/224\")",
            "return": "d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be"
          },
          {
            "title": "Calculate sha2 hash with SHA-384",
            "source": "sha2(\"foobar\", \"SHA-384\")",
            "return": "3c9c30d9f665e74d515c842960d4a451c83a0125fd3de7392d7b37231af10c72ea58aedfcdf89a5765bf902af93ecf06"
          }
        ],
        "pure": true
      }
    }
  }
}
