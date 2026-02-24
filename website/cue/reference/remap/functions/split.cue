{
  "remap": {
    "functions": {
      "split": {
        "anchor": "split",
        "name": "split",
        "category": "String",
        "description": "Splits the `value` string using `pattern`.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to split.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "pattern",
            "description": "The string is split whenever this pattern is matched.",
            "required": true,
            "type": [
              "string",
              "regex"
            ]
          },
          {
            "name": "limit",
            "description": "The maximum number of substrings to return.",
            "required": false,
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
            "If `limit` is specified, the remainder of the string is returned unsplit after `limit` has been reached."
          ]
        },
        "examples": [
          {
            "title": "Split a string (no limit)",
            "source": "split(\"apples and pears and bananas\", \" and \")",
            "return": [
              "apples",
              "pears",
              "bananas"
            ]
          },
          {
            "title": "Split a string (with a limit)",
            "source": "split(\"apples and pears and bananas\", \" and \", limit: 2)",
            "return": [
              "apples",
              "pears and bananas"
            ]
          },
          {
            "title": "Split string",
            "source": "split(\"foobar\", \"b\")",
            "return": [
              "foo",
              "ar"
            ]
          },
          {
            "title": "Split regex",
            "source": "split(\"barbaz\", r'ba')",
            "return": [
              "",
              "r",
              "z"
            ]
          }
        ],
        "pure": true
      }
    }
  }
}
