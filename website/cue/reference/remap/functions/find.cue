{
  "remap": {
    "functions": {
      "find": {
        "anchor": "find",
        "name": "find",
        "category": "String",
        "description": "Determines from left to right the start position of the first found element in `value` that matches `pattern`. Returns `-1` if not found.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to find the pattern in.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "pattern",
            "description": "The regular expression or string pattern to match against.",
            "required": true,
            "type": [
              "string",
              "regex"
            ]
          },
          {
            "name": "from",
            "description": "Offset to start searching.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "0"
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "examples": [
          {
            "title": "Match text",
            "source": "find(\"foobar\", \"bar\")",
            "return": 3
          },
          {
            "title": "Match text at start",
            "source": "find(\"foobar\", \"foo\")",
            "return": 0
          },
          {
            "title": "Match regex",
            "source": "find(\"foobar\", r'b.r')",
            "return": 3
          },
          {
            "title": "No matches",
            "source": "find(\"foobar\", \"baz\")",
            "return": null
          },
          {
            "title": "With an offset",
            "source": "find(\"foobarfoobarfoo\", \"bar\", 4)",
            "return": 9
          }
        ],
        "pure": true
      }
    }
  }
}