{
  "remap": {
    "functions": {
      "map_values": {
        "anchor": "map_values",
        "name": "map_values",
        "category": "Enumerate",
        "description": "Map the values within a collection.\n\nIf `recursive` is enabled, the function iterates into nested\ncollections, using the following rules:\n\n1. Iteration starts at the root.\n2. For every nested collection type:\n   - First return the collection type itself.\n   - Then recurse into the collection, and loop back to item (1)\n     in the list\n   - Any mutation done on a collection *before* recursing into it,\n     are preserved.\n\nThe function uses the function closure syntax to allow mutating\nthe value for each item in the collection.\n\nThe same scoping rules apply to closure blocks as they do for\nregular blocks, meaning, any variable defined in parent scopes\nare accessible, and mutations to those variables are preserved,\nbut any new variables instantiated in the closure block are\nunavailable outside of the block.\n\nCheck out the examples below to learn about the closure syntax.",
        "arguments": [
          {
            "name": "value",
            "description": "The object or array to iterate.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          },
          {
            "name": "recursive",
            "description": "Whether to recursively iterate the collection.",
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
          ]
        },
        "examples": [
          {
            "title": "Upcase values",
            "source": ". = {\n    \"foo\": \"foo\",\n    \"bar\": \"bar\"\n}\nmap_values(.) -> |value| { upcase(value) }\n",
            "return": {
              "foo": "FOO",
              "bar": "BAR"
            }
          },
          {
            "title": "Recursively map object values",
            "source": "val = {\n    \"a\": 1,\n    \"b\": [{ \"c\": 2 }, { \"d\": 3 }],\n    \"e\": { \"f\": 4 }\n}\nmap_values(val, recursive: true) -> |value| {\n    if is_integer(value) { int!(value) + 1 } else { value }\n}\n",
            "return": {
              "a": 2,
              "b": [
                {
                  "c": 3
                },
                {
                  "d": 4
                }
              ],
              "e": {
                "f": 5
              }
            }
          }
        ],
        "pure": true
      }
    }
  }
}
