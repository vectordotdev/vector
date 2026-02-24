{
  "remap": {
    "functions": {
      "decrypt": {
        "anchor": "decrypt",
        "name": "decrypt",
        "category": "Cryptography",
        "description": "Decrypts a string with a symmetric encryption algorithm.\n\nSupported Algorithms:\n\n* AES-256-CFB (key = 32 bytes, iv = 16 bytes)\n* AES-192-CFB (key = 24 bytes, iv = 16 bytes)\n* AES-128-CFB (key = 16 bytes, iv = 16 bytes)\n* AES-256-OFB (key = 32 bytes, iv = 16 bytes)\n* AES-192-OFB  (key = 24 bytes, iv = 16 bytes)\n* AES-128-OFB (key = 16 bytes, iv = 16 bytes)\n* AES-128-SIV (key = 32 bytes, iv = 16 bytes)\n* AES-256-SIV (key = 64 bytes, iv = 16 bytes)\n* Deprecated - AES-256-CTR (key = 32 bytes, iv = 16 bytes)\n* Deprecated - AES-192-CTR (key = 24 bytes, iv = 16 bytes)\n* Deprecated - AES-128-CTR (key = 16 bytes, iv = 16 bytes)\n* AES-256-CTR-LE (key = 32 bytes, iv = 16 bytes)\n* AES-192-CTR-LE (key = 24 bytes, iv = 16 bytes)\n* AES-128-CTR-LE (key = 16 bytes, iv = 16 bytes)\n* AES-256-CTR-BE (key = 32 bytes, iv = 16 bytes)\n* AES-192-CTR-BE (key = 24 bytes, iv = 16 bytes)\n* AES-128-CTR-BE (key = 16 bytes, iv = 16 bytes)\n* AES-256-CBC-PKCS7 (key = 32 bytes, iv = 16 bytes)\n* AES-192-CBC-PKCS7 (key = 24 bytes, iv = 16 bytes)\n* AES-128-CBC-PKCS7 (key = 16 bytes, iv = 16 bytes)\n* AES-256-CBC-ANSIX923 (key = 32 bytes, iv = 16 bytes)\n* AES-192-CBC-ANSIX923 (key = 24 bytes, iv = 16 bytes)\n* AES-128-CBC-ANSIX923 (key = 16 bytes, iv = 16 bytes)\n* AES-256-CBC-ISO7816 (key = 32 bytes, iv = 16 bytes)\n* AES-192-CBC-ISO7816 (key = 24 bytes, iv = 16 bytes)\n* AES-128-CBC-ISO7816 (key = 16 bytes, iv = 16 bytes)\n* AES-256-CBC-ISO10126 (key = 32 bytes, iv = 16 bytes)\n* AES-192-CBC-ISO10126 (key = 24 bytes, iv = 16 bytes)\n* AES-128-CBC-ISO10126 (key = 16 bytes, iv = 16 bytes)\n* CHACHA20-POLY1305 (key = 32 bytes, iv = 12 bytes)\n* XCHACHA20-POLY1305 (key = 32 bytes, iv = 24 bytes)\n* XSALSA20-POLY1305 (key = 32 bytes, iv = 24 bytes)",
        "arguments": [
          {
            "name": "ciphertext",
            "description": "The string in raw bytes (not encoded) to decrypt.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "algorithm",
            "description": "The algorithm to use.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "key",
            "description": "The key in raw bytes (not encoded) for decryption. The length must match the algorithm requested.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "iv",
            "description": "The IV in raw bytes (not encoded) for decryption. The length must match the algorithm requested.\nA new IV should be generated for every message. You can use `random_bytes` to generate a cryptographically secure random value.\nThe value should match the one used during encryption.",
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
        "internal_failure_reasons": [
          "`algorithm` is not a supported algorithm.",
          "`key` length does not match the key size required for the algorithm specified.",
          "`iv` length does not match the `iv` size required for the algorithm specified."
        ],
        "examples": [
          {
            "title": "Decrypt value using AES-256-CFB",
            "source": "iv = \"0123456789012345\"\nkey = \"01234567890123456789012345678912\"\nciphertext = decode_base64!(\"c/dIOA==\")\ndecrypt!(ciphertext, \"AES-256-CFB\", key: key, iv: iv)\n",
            "return": "data"
          },
          {
            "title": "Decrypt value using AES-128-CBC-PKCS7",
            "source": "iv = decode_base64!(\"fVEIRkIiczCRWNxaarsyxA==\")\nkey = \"16_byte_keyxxxxx\"\nciphertext = decode_base64!(\"5fLGcu1VHdzsPcGNDio7asLqE1P43QrVfPfmP4i4zOU=\")\ndecrypt!(ciphertext, \"AES-128-CBC-PKCS7\", key: key, iv: iv)\n",
            "return": "super_secret_message"
          }
        ],
        "pure": true
      }
    }
  }
}
