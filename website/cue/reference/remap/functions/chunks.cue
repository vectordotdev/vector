{
  "remap": {
    "functions": {
      "chunks": {
        "anchor": "chunks",
        "name": "chunks",
        "category": "Array",
        "description": "Chunks `value` into slices of length `chunk_size` bytes.",
        "arguments": [
          {
            "name": "value",
            "description": "The array of bytes to split.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "chunk_size",
            "description": "The desired length of each chunk in bytes. This may be constrained by the host platform architecture.",
            "required": true,
            "type": [
              "integer"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ],
          "rules": [
            "`chunks` is considered fallible if the supplied `chunk_size` is an expression, and infallible if it's a literal integer."
          ]
        },
        "internal_failure_reasons": [
          "`chunk_size` must be at least 1 byte.",
          "`chunk_size` is too large."
        ],
        "examples": [
          {
            "title": "Split a string into chunks",
            "source": "chunks(\"abcdefgh\", 4)",
            "return": [
              "abcd",
              "efgh"
            ]
          },
          {
            "title": "Chunks do not respect unicode code point boundaries",
            "source": "chunks(\"ab你好\", 4)",
            "return": [
              "ab�",
              "�好"
            ]
          }
        ],
        "pure": true
      }
    }
  }
}
