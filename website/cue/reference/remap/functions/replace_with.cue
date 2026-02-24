{
  "remap": {
    "functions": {
      "replace_with": {
        "anchor": "replace_with",
        "name": "replace_with",
        "category": "String",
        "description": "Replaces all matching instances of `pattern` using a closure.\n\nThe `pattern` argument accepts a regular expression that can use capture groups.\n\nThe function uses the function closure syntax to compute the replacement values.\n\nThe closure takes a single parameter, which is an array, where the first item is always\npresent and contains the entire string that matched `pattern`. The items from index one on\ncontain the capture groups of the corresponding index. If a capture group is optional, the\nvalue may be null if it didn't match.\n\nThe value returned by the closure must be a string and will replace the section of\nthe input that was matched.\n\nThis returns a new string with the replacements, the original string is not mutated.",
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
            "description": "Replace all matches of this pattern. Must be a regular expression.",
            "required": true,
            "type": [
              "regex"
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
            "title": "Capitalize words",
            "source": "replace_with(\"apples and bananas\", r'\\b(\\w)(\\w*)') -> |match| {\n    upcase!(match.captures[0]) + string!(match.captures[1])\n}\n",
            "return": "Apples And Bananas"
          },
          {
            "title": "Replace with hash",
            "source": "replace_with(\"email from test@example.com\", r'\\w+@example.com') -> |match| {\n    sha2(match.string, variant: \"SHA-512/224\")\n}\n",
            "return": "email from adf6e1bc4415d24912bd93072ad34ef825a7b6eb3bf53f68def1fc17"
          },
          {
            "title": "Replace first instance",
            "source": "replace_with(\"Apples and Apples\", r'(?i)apples|cones', count: 1) -> |match| {\n    \"Pine\" + downcase(match.string)\n}\n",
            "return": "Pineapples and Apples"
          },
          {
            "title": "Named capture group",
            "source": "replace_with(\"level=error A message\", r'level=(?P<level>\\w+)') -> |match| {\n    lvl = upcase!(match.level)\n    \"[{{lvl}}]\"\n}\n",
            "return": "[ERROR] A message"
          },
          {
            "title": "Replace with processed capture group",
            "source": "replace_with(s'Got message: {\"msg\": \"b\"}', r'message: (\\{.*\\})') -> |m| {\n    to_string!(parse_json!(m.captures[0]).msg)\n}\n",
            "return": "Got b"
          },
          {
            "title": "Replace with optional capture group",
            "source": "replace_with(\"bar of chocolate and bar of gold\", r'bar( of gold)?') -> |m| {\n    if m.captures[0] == null { \"pile\" } else { \"money\" }\n}\n",
            "return": "pile of chocolate and money"
          }
        ],
        "pure": true
      }
    }
  }
}
