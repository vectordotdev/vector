{
  "remap": {
    "functions": {
      "for_each": {
        "anchor": "for_each",
        "name": "for_each",
        "category": "Enumerate",
        "description": "Iterate over a collection.\n\nThis function currently *does not* support recursive iteration.\n\nThe function uses the \"function closure syntax\" to allow reading\nthe key/value or index/value combination for each item in the\ncollection.\n\nThe same scoping rules apply to closure blocks as they do for\nregular blocks. This means that any variable defined in parent scopes\nis accessible, and mutations to those variables are preserved,\nbut any new variables instantiated in the closure block are\nunavailable outside of the block.\n\nSee the examples below to learn about the closure syntax.",
        "arguments": [
          {
            "name": "value",
            "description": "The array or object to iterate.",
            "required": true,
            "type": [
              "object",
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "null"
          ]
        },
        "examples": [
          {
            "title": "Tally elements",
            "source": ".tags = [\"foo\", \"bar\", \"foo\", \"baz\"]\ntally = {}\nfor_each(array(.tags)) -> |_index, value| {\n    count = int(get!(tally, [value])) ?? 0\n    tally = set!(tally, [value], count + 1)\n}\ntally\n",
            "return": {
              "bar": 1,
              "baz": 1,
              "foo": 2
            }
          },
          {
            "title": "Iterate over an object",
            "source": "count = 0\nfor_each({ \"a\": 1, \"b\": 2 }) -> |_key, value| {\n    count = count + value\n}\ncount\n",
            "return": 3
          },
          {
            "title": "Iterate over an array",
            "source": "count = 0\nfor_each([1, 2, 3]) -> |index, value| {\n    count = count + index + value\n}\ncount\n",
            "return": 9
          }
        ],
        "pure": true
      }
    }
  }
}
