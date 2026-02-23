{
  "remap": {
    "functions": {
      "decode_mime_q": {
        "anchor": "decode_mime_q",
        "name": "decode_mime_q",
        "category": "Codec",
        "description": "Replaces q-encoded or base64-encoded [encoded-word](https://datatracker.ietf.org/doc/html/rfc2047#section-2) substrings in the `value` with their original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The string with [encoded-words](https://datatracker.ietf.org/doc/html/rfc2047#section-2) to decode.",
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
          "`value` has invalid encoded [encoded-word](https://datatracker.ietf.org/doc/html/rfc2047#section-2) string."
        ],
        "examples": [
          {
            "title": "Decode single encoded-word",
            "source": "decode_mime_q!(\"=?utf-8?b?SGVsbG8sIFdvcmxkIQ==?=\")",
            "return": "Hello, World!"
          },
          {
            "title": "Embedded",
            "source": "decode_mime_q!(\"From: =?utf-8?b?SGVsbG8sIFdvcmxkIQ==?= <=?utf-8?q?hello=5Fworld=40example=2ecom?=>\")",
            "return": "From: Hello, World! <hello_world@example.com>"
          },
          {
            "title": "Without charset",
            "source": "decode_mime_q!(\"?b?SGVsbG8sIFdvcmxkIQ==\")",
            "return": "Hello, World!"
          }
        ],
        "pure": true
      }
    }
  }
}