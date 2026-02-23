{
  "remap": {
    "functions": {
      "to_regex": {
        "anchor": "to_regex",
        "name": "to_regex",
        "category": "Coerce",
        "description": "Coerces the `value` into a regex.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a regex.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "regex"
          ],
          "rules": [
            "If `value` is a string that contains a valid regex, returns the regex constructed with this string."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a string."
        ],
        "examples": [
          {
            "title": "Coerce to a regex",
            "source": "to_regex!(\"^foo$\")",
            "return": "r'^foo$'"
          }
        ],
        "notices": [
          "Compiling a regular expression is an expensive operation and can limit Vector's\nthroughput. Don't use this function unless you are absolutely sure there is no other\nway!"
        ],
        "pure": true
      }
    }
  }
}