{
  "remap": {
    "functions": {
      "contains_all": {
        "anchor": "contains_all",
        "name": "contains_all",
        "category": "String",
        "description": "Determines whether the `value` string contains all the specified `substrings`.",
        "arguments": [
          {
            "name": "value",
            "description": "The text to search.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "substrings",
            "description": "An array of substrings to search for in `value`.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "case_sensitive",
            "description": "Whether the match should be case sensitive.",
            "required": false,
            "type": [
              "boolean"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "examples": [
          {
            "title": "String contains all with default parameters (case sensitive)",
            "source": "contains_all(\"The NEEDLE in the Haystack\", [\"NEEDLE\", \"Haystack\"])",
            "return": true
          },
          {
            "title": "String doesn't contain all with default parameters (case sensitive)",
            "source": "contains_all(\"The NEEDLE in the Haystack\", [\"needle\", \"Haystack\"])",
            "return": false
          },
          {
            "title": "String contains all (case insensitive)",
            "source": "contains_all(\"The NEEDLE in the HaYsTaCk\", [\"nEeDlE\", \"haystack\"], case_sensitive: false)",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}
