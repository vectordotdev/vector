{
  "remap": {
    "functions": {
      "replace": {
        "anchor": "replace",
        "name": "replace",
        "category": "String",
        "description": "Replaces all matching instances of `pattern` in `value`.\n\nThe `pattern` argument accepts regular expression capture groups.\n\n**Note when using capture groups**:\n- You will need to escape the `$` by using `$$` to avoid Vector interpreting it as an\n  [environment variable when loading configuration](/docs/reference/environment_variables/#escaping)\n- If you want a literal `$` in the replacement pattern, you will also need to escape this\n  with `$$`. When combined with environment variable interpolation in config files this\n  means you will need to use `$$$$` to have a literal `$` in the replacement pattern.",
        "arguments": [
          {
            "name": "value",
            "description": "The original string.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "pattern",
            "description": "Replace all matches of this pattern. Can be a static string or a regular expression.",
            "required": true,
            "type": [
              "string",
              "regex"
            ]
          },
          {
            "name": "with",
            "description": "The string that the matches are replaced with.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "count",
            "description": "The maximum number of replacements to perform. `-1` means replace all matches.",
            "required": false,
            "type": [
              "integer"
            ],
            "default": "-1"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Replace literal text",
            "source": "replace(\"Apples and Bananas\", \"and\", \"not\")",
            "return": "Apples not Bananas"
          },
          {
            "title": "Replace using regular expression",
            "source": "replace(\"Apples and Bananas\", r'(?i)bananas', \"Pineapples\")",
            "return": "Apples and Pineapples"
          },
          {
            "title": "Replace first instance",
            "source": "replace(\"Bananas and Bananas\", \"Bananas\", \"Pineapples\", count: 1)",
            "return": "Pineapples and Bananas"
          },
          {
            "title": "Replace with capture groups",
            "source": "# Note that in the context of Vector configuration files, an extra `$` escape character is required\n# (i.e. `$$num`) to avoid interpreting `num` as an environment variable.\nreplace(\"foo123bar\", r'foo(?P<num>\\d+)bar', \"$num\")\n",
            "return": "123"
          },
          {
            "title": "Replace all",
            "source": "replace(\"foobar\", \"o\", \"i\")",
            "return": "fiibar"
          }
        ],
        "pure": true
      }
    }
  }
}