{
  "remap": {
    "functions": {
      "to_int": {
        "anchor": "to_int",
        "name": "to_int",
        "category": "Coerce",
        "description": "Coerces the `value` into an integer.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to an integer.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ],
          "rules": [
            "If `value` is an integer, it will be returned as-is.",
            "If `value` is a float, it will be truncated to its integer portion.",
            "If `value` is a string, it must be the string representation of an integer or else an error is raised.",
            "If `value` is a boolean, `0` is returned for `false` and `1` is returned for `true`.",
            "If `value` is a timestamp, a [Unix timestamp](https://en.wikipedia.org/wiki/Unix_time) (in seconds) is returned.",
            "If `value` is null, `0` is returned."
          ]
        },
        "internal_failure_reasons": [
          "`value` is a string but the text is not an integer.",
          "`value` is not a string, int, or timestamp."
        ],
        "examples": [
          {
            "title": "Coerce to an int (string)",
            "source": "to_int!(\"2\")",
            "return": 2
          },
          {
            "title": "Coerce to an int (timestamp)",
            "source": "to_int(t'2020-12-30T22:20:53.824727Z')",
            "return": 1609366853
          },
          {
            "title": "Integer",
            "source": "to_int(5)",
            "return": 5
          },
          {
            "title": "Float",
            "source": "to_int(5.6)",
            "return": 5
          },
          {
            "title": "True",
            "source": "to_int(true)",
            "return": 1
          },
          {
            "title": "False",
            "source": "to_int(false)",
            "return": 0
          },
          {
            "title": "Null",
            "source": "to_int(null)",
            "return": 0
          },
          {
            "title": "Invalid string",
            "source": "to_int!(s'foobar')",
            "raises": "function call error for \"to_int\" at (0:18): Invalid integer \"foobar\": invalid digit found in string"
          },
          {
            "title": "Array",
            "source": "to_int!([])",
            "raises": "function call error for \"to_int\" at (0:11): unable to coerce array into integer"
          },
          {
            "title": "Object",
            "source": "to_int!({})",
            "raises": "function call error for \"to_int\" at (0:11): unable to coerce object into integer"
          },
          {
            "title": "Regex",
            "source": "to_int!(r'foo')",
            "raises": "function call error for \"to_int\" at (0:15): unable to coerce regex into integer"
          }
        ],
        "pure": true
      }
    }
  }
}
