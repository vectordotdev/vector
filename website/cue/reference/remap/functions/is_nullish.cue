{
  "remap": {
    "functions": {
      "is_nullish": {
        "anchor": "is_nullish",
        "name": "is_nullish",
        "category": "Type",
        "description": "Determines whether `value` is nullish. Returns `true` if the specified `value` is `null`, an empty string, a string containing only whitespace, or the string `\"-\"`. Returns `false` otherwise.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to check for nullishness, for example, a useless value.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ],
          "rules": [
            "Returns `true` if `value` is `null`.",
            "Returns `true` if `value` is `\"-\"`.",
            "Returns `true` if `value` is whitespace as defined by [Unicode `White_Space` property](https://en.wikipedia.org/wiki/Unicode_character_property#Whitespace).",
            "Returns `false` if `value` is anything else."
          ]
        },
        "examples": [
          {
            "title": "Null detection (blank string)",
            "source": "is_nullish(\"\")",
            "return": true
          },
          {
            "title": "Null detection (dash string)",
            "source": "is_nullish(\"-\")",
            "return": true
          },
          {
            "title": "Null detection (whitespace)",
            "source": "is_nullish(\"\n  \n\")",
            "return": true
          },
          {
            "title": "Null",
            "source": "is_nullish(null)",
            "return": true
          }
        ],
        "notices": [
          "This function behaves inconsistently: it returns `false` for empty arrays (`[]`) and\nobjects (`{}`), but `true` for empty strings (`\"\"`) and `null`."
        ],
        "pure": true
      }
    }
  }
}