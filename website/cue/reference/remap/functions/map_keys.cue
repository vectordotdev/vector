{
  "remap": {
    "functions": {
      "map_keys": {
        "anchor": "map_keys",
        "name": "map_keys",
        "category": "Enumerate",
        "description": "Map the keys within an object.\n\nIf `recursive` is enabled, the function iterates into nested\nobjects, using the following rules:\n\n1. Iteration starts at the root.\n2. For every nested object type:\n   - First return the key of the object type itself.\n   - Then recurse into the object, and loop back to item (1)\n     in this list.\n   - Any mutation done on a nested object *before* recursing into\n     it, are preserved.\n3. For every nested array type:\n   - First return the key of the array type itself.\n   - Then find all objects within the array, and apply item (2)\n     to each individual object.\n\nThe above rules mean that `map_keys` with\n`recursive` enabled finds *all* keys in the target,\nregardless of whether nested objects are nested inside arrays.\n\nThe function uses the function closure syntax to allow reading\nthe key for each item in the object.\n\nThe same scoping rules apply to closure blocks as they do for\nregular blocks. This means that any variable defined in parent scopes\nis accessible, and mutations to those variables are preserved,\nbut any new variables instantiated in the closure block are\nunavailable outside of the block.\n\nSee the examples below to learn about the closure syntax.",
        "arguments": [
          {
            "name": "value",
            "description": "The object to iterate.",
            "required": true,
            "type": [
              "object"
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
            "object"
          ]
        },
        "examples": [
          {
            "title": "Upcase keys",
            "source": ". = {\n    \"foo\": \"foo\",\n    \"bar\": \"bar\",\n    \"baz\": {\"nested key\": \"val\"}\n}\nmap_keys(.) -> |key| { upcase(key) }\n",
            "return": {
              "FOO": "foo",
              "BAR": "bar",
              "BAZ": {
                "nested key": "val"
              }
            }
          },
          {
            "title": "De-dot keys",
            "source": ". = {\n    \"labels\": {\n        \"app.kubernetes.io/name\": \"mysql\"\n    }\n}\nmap_keys(., recursive: true) -> |key| { replace(key, \".\", \"_\") }\n",
            "return": {
              "labels": {
                "app_kubernetes_io/name": "mysql"
              }
            }
          },
          {
            "title": "Recursively map object keys",
            "source": "val = {\n    \"a\": 1,\n    \"b\": [{ \"c\": 2 }, { \"d\": 3 }],\n    \"e\": { \"f\": 4 }\n}\nmap_keys(val, recursive: true) -> |key| { upcase(key) }\n",
            "return": {
              "A": 1,
              "B": [
                {
                  "C": 2
                },
                {
                  "D": 3
                }
              ],
              "E": {
                "F": 4
              }
            }
          }
        ],
        "pure": true
      }
    }
  }
}
