{
  "remap": {
    "functions": {
      "to_bool": {
        "anchor": "to_bool",
        "name": "to_bool",
        "category": "Coerce",
        "description": "Coerces the `value` into a boolean.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to convert to a Boolean.",
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
            "If `value` is `\"true\"`, `\"t\"`, `\"yes\"`, or `\"y\"`, `true` is returned.",
            "If `value` is `\"false\"`, `\"f\"`, `\"no\"`, `\"n\"`, or `\"0\"`, `false` is returned.",
            "If `value` is `0.0`, `false` is returned, otherwise `true` is returned.",
            "If `value` is `0`, `false` is returned, otherwise `true` is returned.",
            "If `value` is `null`, `false` is returned.",
            "If `value` is a Boolean, it's returned unchanged."
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a supported boolean representation."
        ],
        "examples": [
          {
            "title": "Coerce to a Boolean (string)",
            "source": "to_bool!(\"yes\")",
            "return": true
          },
          {
            "title": "Coerce to a Boolean (float)",
            "source": "to_bool(0.0)",
            "return": false
          },
          {
            "title": "Coerce to a Boolean (int)",
            "source": "to_bool(0)",
            "return": false
          },
          {
            "title": "Coerce to a Boolean (null)",
            "source": "to_bool(null)",
            "return": false
          },
          {
            "title": "Coerce to a Boolean (Boolean)",
            "source": "to_bool(true)",
            "return": true
          },
          {
            "title": "Integer (other)",
            "source": "to_bool(2)",
            "return": true
          },
          {
            "title": "Float (other)",
            "source": "to_bool(5.6)",
            "return": true
          },
          {
            "title": "False",
            "source": "to_bool(false)",
            "return": false
          },
          {
            "title": "True string",
            "source": "to_bool!(s'true')",
            "return": true
          },
          {
            "title": "Y string",
            "source": "to_bool!(s'y')",
            "return": true
          },
          {
            "title": "Non-zero integer string",
            "source": "to_bool!(s'1')",
            "return": true
          },
          {
            "title": "False string",
            "source": "to_bool!(s'false')",
            "return": false
          },
          {
            "title": "No string",
            "source": "to_bool!(s'no')",
            "return": false
          },
          {
            "title": "N string",
            "source": "to_bool!(s'n')",
            "return": false
          },
          {
            "title": "Invalid string",
            "source": "to_bool!(s'foobar')",
            "raises": "function call error for \"to_bool\" at (0:19): Invalid boolean value \"foobar\""
          },
          {
            "title": "Timestamp",
            "source": "to_bool!(t'2020-01-01T00:00:00Z')",
            "raises": "function call error for \"to_bool\" at (0:33): unable to coerce timestamp into boolean"
          },
          {
            "title": "Array",
            "source": "to_bool!([])",
            "raises": "function call error for \"to_bool\" at (0:12): unable to coerce array into boolean"
          },
          {
            "title": "Object",
            "source": "to_bool!({})",
            "raises": "function call error for \"to_bool\" at (0:12): unable to coerce object into boolean"
          },
          {
            "title": "Regex",
            "source": "to_bool!(r'foo')",
            "raises": "function call error for \"to_bool\" at (0:16): unable to coerce regex into boolean"
          }
        ],
        "pure": true
      }
    }
  }
}