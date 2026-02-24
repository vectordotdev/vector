{
  "remap": {
    "functions": {
      "del": {
        "anchor": "del",
        "name": "del",
        "category": "Path",
        "description": "Removes the field specified by the static `path` from the target.\n\nFor dynamic path deletion, see the `remove` function.",
        "arguments": [
          {
            "name": "target",
            "description": "The path of the field to delete",
            "required": true,
            "type": [
              "any"
            ]
          },
          {
            "name": "compact",
            "description": "After deletion, if `compact` is `true` and there is an empty object or array left,\nthe empty object or array is also removed, cascading up to the root. This only\napplies to the path being deleted, and any parent paths.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "any"
          ],
          "rules": [
            "Returns the value of the field being deleted. Returns `null` if the field doesn't exist."
          ]
        },
        "examples": [
          {
            "title": "Delete a field",
            "source": ". = { \"foo\": \"bar\" }\ndel(.foo)\n",
            "return": "bar"
          },
          {
            "title": "Rename a field",
            "source": ". = { \"old\": \"foo\" }\n.new = del(.old)\n.\n",
            "return": {
              "new": "foo"
            }
          },
          {
            "title": "Returns null for unknown field",
            "source": "del({\"foo\": \"bar\"}.baz)",
            "return": null
          },
          {
            "title": "External target",
            "source": ". = { \"foo\": true, \"bar\": 10 }\ndel(.foo)\n.\n",
            "return": {
              "bar": 10
            }
          },
          {
            "title": "Delete field from variable",
            "source": "var = { \"foo\": true, \"bar\": 10 }\ndel(var.foo)\nvar\n",
            "return": {
              "bar": 10
            }
          },
          {
            "title": "Delete object field",
            "source": "var = { \"foo\": {\"nested\": true}, \"bar\": 10 }\ndel(var.foo.nested, false)\nvar\n",
            "return": {
              "foo": {},
              "bar": 10
            }
          },
          {
            "title": "Compact object field",
            "source": "var = { \"foo\": {\"nested\": true}, \"bar\": 10 }\ndel(var.foo.nested, true)\nvar\n",
            "return": {
              "bar": 10
            }
          }
        ],
        "notices": [
          "The `del` function _modifies the current event in place_ and returns the value of the deleted field."
        ],
        "pure": false
      }
    }
  }
}
