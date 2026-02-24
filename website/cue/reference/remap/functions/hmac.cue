{
  "remap": {
    "functions": {
      "hmac": {
        "anchor": "hmac",
        "name": "hmac",
        "category": "Cryptography",
        "description": "Calculates a [HMAC](https://en.wikipedia.org/wiki/HMAC) of the `value` using the given `key`.\nThe hashing `algorithm` used can be optionally specified.\n\nFor most use cases, the resulting bytestream should be encoded into a hex or base64\nstring using either [encode_base16](/docs/reference/vrl/functions/#encode_base16) or\n[encode_base64](/docs/reference/vrl/functions/#encode_base64).\n\nThis function is infallible if either the default `algorithm` value or a recognized-valid compile-time\n`algorithm` string literal is used. Otherwise, it is fallible.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to calculate the HMAC for.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "key",
            "description": "The string to use as the cryptographic key.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "algorithm",
            "description": "The hashing algorithm to use.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "SHA1": "SHA1 algorithm",
              "SHA-224": "SHA-224 algorithm",
              "SHA-256": "SHA-256 algorithm",
              "SHA-384": "SHA-384 algorithm",
              "SHA-512": "SHA-512 algorithm"
            },
            "default": "SHA-256"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Calculate message HMAC (defaults: SHA-256), encoding to a base64 string",
            "source": "encode_base64(hmac(\"Hello there\", \"super-secret-key\"))",
            "return": "eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI="
          },
          {
            "title": "Calculate message HMAC using SHA-224, encoding to a hex-encoded string",
            "source": "encode_base16(hmac(\"Hello there\", \"super-secret-key\", algorithm: \"SHA-224\"))",
            "return": "42fccbc2b7d22a143b92f265a8046187558a94d11ddbb30622207e90"
          },
          {
            "title": "Calculate message HMAC using SHA1, encoding to a base64 string",
            "source": "encode_base64(hmac(\"Hello there\", \"super-secret-key\", algorithm: \"SHA1\"))",
            "return": "MiyBIHO8Set9+6crALiwkS0yFPE="
          },
          {
            "title": "Calculate message HMAC using a variable hash algorithm",
            "source": ".hash_algo = \"SHA-256\"\nhmac_bytes, err = hmac(\"Hello there\", \"super-secret-key\", algorithm: .hash_algo)\nif err == null {\n    .hmac = encode_base16(hmac_bytes)\n}\n",
            "return": "78b184f1832f8aff3934f5e0212454671b2d04d494e3b25075c5e45167029662"
          }
        ],
        "pure": true
      }
    }
  }
}
