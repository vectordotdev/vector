{
  "remap": {
    "functions": {
      "to_float": {
        "anchor": "to_float",
        "name": "to_float",
        "category": "Coerce",
        "description": "Coerces the `value` into a float.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a float. Must be convertible to a float, otherwise an error is raised.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "float"
          ],
          "rules": [
            "If `value` is a float, it will be returned as-is.",
            "If `value` is an integer, it will be returned as as a float.",
            "If `value` is a string, it must be the string representation of an float or else an error is raised.",
            "If `value` is a boolean, `0.0` is returned for `false` and `1.0` is returned for `true`.",
            "If `value` is a timestamp, a [Unix timestamp](https://en.wikipedia.org/wiki/Unix_time) with fractional seconds is returned."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a supported float representation."
        ],
        "examples": [
          {
            "title": "Coerce to a float",
            "source": "to_float!(\"3.145\")",
            "return": 3.145
          },
          {
            "title": "Coerce to a float (timestamp)",
            "source": "to_float(t'2020-12-30T22:20:53.824727Z')",
            "return": 1609366853.824727
          },
          {
            "title": "Integer",
            "source": "to_float(5)",
            "return": 5.0
          },
          {
            "title": "Float",
            "source": "to_float(5.6)",
            "return": 5.6
          },
          {
            "title": "True",
            "source": "to_float(true)",
            "return": 1.0
          },
          {
            "title": "False",
            "source": "to_float(false)",
            "return": 0.0
          },
          {
            "title": "Null",
            "source": "to_float(null)",
            "return": 0.0
          },
          {
            "title": "Invalid string",
            "source": "to_float!(s'foobar')",
            "raises": "function call error for \"to_float\" at (0:20): Invalid floating point number \"foobar\": invalid float literal"
          },
          {
            "title": "Array",
            "source": "to_float!([])",
            "raises": "function call error for \"to_float\" at (0:13): unable to coerce array into float"
          },
          {
            "title": "Object",
            "source": "to_float!({})",
            "raises": "function call error for \"to_float\" at (0:13): unable to coerce object into float"
          },
          {
            "title": "Regex",
            "source": "to_float!(r'foo')",
            "raises": "function call error for \"to_float\" at (0:17): unable to coerce regex into float"
          }
        ],
        "pure": true
      }
    }
  }
}
