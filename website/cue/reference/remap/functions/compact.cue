{
  "remap": {
    "functions": {
      "compact": {
        "anchor": "compact",
        "name": "compact",
        "category": "Enumerate",
        "description": "Compacts the `value` by removing empty values, where empty values are defined using the available parameters.",
        "arguments": [
          {
            "name": "value",
            "description": "The object or array to compact.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "recursive",
            "description": "Whether the compaction be recursive.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "null",
            "description": "Whether null should be treated as an empty value.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "string",
            "description": "Whether an empty string should be treated as an empty value.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "object",
            "description": "Whether an empty object should be treated as an empty value.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "array",
            "description": "Whether an empty array should be treated as an empty value.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          },
          {
            "name": "nullish",
            "description": "Tests whether the value is \"nullish\" as defined by the [`is_nullish`](#is_nullish) function.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "false"
          }
        ],
        "return": {
          "types": [
            "object",
            "array"
          ],
          "rules": [
            "The return type matches the `value` type."
          ]
        },
        "examples": [
          {
            "title": "Compact an object with default parameters",
            "source": "compact({\"field1\": 1, \"field2\": \"\", \"field3\": [], \"field4\": null})",
            "return": {
              "field1": 1
            }
          },
          {
            "title": "Compact an array with default parameters",
            "source": "compact([\"foo\", \"bar\", \"\", null, [], \"buzz\"])",
            "return": [
              "foo",
              "bar",
              "buzz"
            ]
          },
          {
            "title": "Compact an array using nullish",
            "source": "compact([\"-\", \"   \", \"\\n\", null, true], nullish: true)",
            "return": [
              true
            ]
          },
          {
            "title": "Compact a complex object with default parameters",
            "source": "compact({ \"a\": {}, \"b\": null, \"c\": [null], \"d\": \"\", \"e\": \"-\", \"f\": true })",
            "return": {
              "e": "-",
              "f": true
            }
          },
          {
            "title": "Compact a complex object using null: false",
            "source": "compact({ \"a\": {}, \"b\": null, \"c\": [null], \"d\": \"\", \"e\": \"-\", \"f\": true }, null: false)",
            "return": {
              "b": null,
              "c": [
                null
              ],
              "e": "-",
              "f": true
            }
          }
        ],
        "pure": true
      }
    }
  }
}