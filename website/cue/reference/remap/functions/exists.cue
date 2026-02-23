{
  "remap": {
    "functions": {
      "exists": {
        "anchor": "exists",
        "name": "exists",
        "category": "Path",
        "description": "Checks whether the `path` exists for the target.\n\nThis function distinguishes between a missing path\nand a path with a `null` value. A regular path lookup,\nsuch as `.foo`, cannot distinguish between the two cases\nsince it always returns `null` if the path doesn't exist.",
        "arguments": [
          {
            "name": "field",
            "description": "The path of the field to check.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "examples": [
          {
            "title": "Exists (field)",
            "source": ". = { \"field\": 1 }\nexists(.field)\n",
            "return": true
          },
          {
            "title": "Exists (array element)",
            "source": ". = { \"array\": [1, 2, 3] }\nexists(.array[2])\n",
            "return": true
          },
          {
            "title": "Does not exist (field)",
            "source": "exists({ \"foo\": \"bar\"}.baz)",
            "return": false
          }
        ],
        "pure": true
      }
    }
  }
}