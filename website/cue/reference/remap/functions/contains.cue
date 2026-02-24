{
  "remap": {
    "functions": {
      "contains": {
        "anchor": "contains",
        "name": "contains",
        "category": "String",
        "description": "Determines whether the `value` string contains the specified `substring`.",
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
            "name": "substring",
            "description": "The substring to search for in `value`.",
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
            "title": "String contains with default parameters (case sensitive)",
            "source": "contains(\"banana\", \"AnA\")",
            "return": false
          },
          {
            "title": "String contains (case insensitive)",
            "source": "contains(\"banana\", \"AnA\", case_sensitive: false)",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}
