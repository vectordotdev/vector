{
  "remap": {
    "functions": {
      "starts_with": {
        "anchor": "starts_with",
        "name": "starts_with",
        "category": "String",
        "description": "Determines whether `value` begins with `substring`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to search.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "substring",
            "description": "The substring that the `value` must start with.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "case_sensitive",
            "description": "Whether the match should be case sensitive.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "examples": [
          {
            "title": "String starts with (case sensitive)",
            "source": "starts_with(\"The Needle In The Haystack\", \"The Needle\")",
            "return": true
          },
          {
            "title": "String starts with (case insensitive)",
            "source": "starts_with(\"The Needle In The Haystack\", \"the needle\", case_sensitive: false)",
            "return": true
          },
          {
            "title": "String starts with (case sensitive failure)",
            "source": "starts_with(\"foobar\", \"F\")",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}
