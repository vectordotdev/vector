{
  "remap": {
    "functions": {
      "truncate": {
        "anchor": "truncate",
        "name": "truncate",
        "category": "String",
        "description": "Truncates the `value` string up to the `limit` number of characters.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to truncate.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "limit",
            "description": "The number of characters to truncate the string after.",
            "required": true,
            "type": [
              "integer"
            ]
          },
          {
            "name": "suffix",
            "description": "A custom suffix (`...`) is appended to truncated strings.\nIf `ellipsis` is set to `true`, this parameter is ignored for backwards compatibility.",
            "required": false,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ],
          "rules": [
            "The string is returned unchanged its length is less than `limit`."
          ]
        },
        "examples": [
          {
            "title": "Truncate a string",
            "source": "truncate(\"A rather long sentence.\", limit: 11, suffix: \"...\")",
            "return": "A rather lo..."
          },
          {
            "title": "Truncate a string (custom suffix)",
            "source": "truncate(\"A rather long sentence.\", limit: 11, suffix: \"[TRUNCATED]\")",
            "return": "A rather lo[TRUNCATED]"
          },
          {
            "title": "Truncate",
            "source": "truncate(\"foobar\", 3)",
            "return": "foo"
          }
        ],
        "pure": true
      }
    }
  }
}
