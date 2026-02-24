{
  "remap": {
    "functions": {
      "to_string": {
        "anchor": "to_string",
        "name": "to_string",
        "category": "Coerce",
        "description": "Coerces the `value` into a string.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a string.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ],
          "rules": [
            "If `value` is an integer or float, returns the string representation.",
            "If `value` is a boolean, returns `\"true\"` or `\"false\"`.",
            "If `value` is a timestamp, returns an [RFC 3339](\\(urls.rfc3339)) representation.",
            "If `value` is a null, returns `\"\"`."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not an integer, float, boolean, string, timestamp, or null."
        ],
        "examples": [
          {
            "title": "Coerce to a string (Boolean)",
            "source": "to_string(true)",
            "return": "s'true'"
          },
          {
            "title": "Coerce to a string (int)",
            "source": "to_string(52)",
            "return": "s'52'"
          },
          {
            "title": "Coerce to a string (float)",
            "source": "to_string(52.2)",
            "return": "s'52.2'"
          },
          {
            "title": "String",
            "source": "to_string(s'foo')",
            "return": "foo"
          },
          {
            "title": "False",
            "source": "to_string(false)",
            "return": "s'false'"
          },
          {
            "title": "Null",
            "source": "to_string(null)",
            "return": ""
          },
          {
            "title": "Timestamp",
            "source": "to_string(t'2020-01-01T00:00:00Z')",
            "return": "2020-01-01T00:00:00Z"
          },
          {
            "title": "Array",
            "source": "to_string!([])",
            "raises": "function call error for \"to_string\" at (0:14): unable to coerce array into string"
          },
          {
            "title": "Object",
            "source": "to_string!({})",
            "raises": "function call error for \"to_string\" at (0:14): unable to coerce object into string"
          },
          {
            "title": "Regex",
            "source": "to_string!(r'foo')",
            "raises": "function call error for \"to_string\" at (0:18): unable to coerce regex into string"
          }
        ],
        "pure": true
      }
    }
  }
}
