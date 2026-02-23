{
  "remap": {
    "functions": {
      "strip_whitespace": {
        "anchor": "strip_whitespace",
        "name": "strip_whitespace",
        "category": "String",
        "description": "Strips whitespace from the start and end of `value`, where whitespace is defined by the [Unicode `White_Space` property](https://en.wikipedia.org/wiki/Unicode_character_property#Whitespace).",
        "arguments": [
          {
            "name": "value",
            "description": "The string to trim.",
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
            "title": "Strip whitespace",
            "source": "strip_whitespace(\"  A sentence.  \")",
            "return": "A sentence."
          },
          {
            "title": "Start whitespace",
            "source": "strip_whitespace(\"  foobar\")",
            "return": "foobar"
          },
          {
            "title": "End whitespace",
            "source": "strip_whitespace(\"foo bar  \")",
            "return": "foo bar"
          },
          {
            "title": "Newlines",
            "source": "strip_whitespace(\"\\n\\nfoo bar\\n  \")",
            "return": "foo bar"
          }
        ],
        "pure": true
      }
    }
  }
}