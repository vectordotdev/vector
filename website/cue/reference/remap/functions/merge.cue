{
  "remap": {
    "functions": {
      "merge": {
        "anchor": "merge",
        "name": "merge",
        "category": "Object",
        "description": "Merges the `from` object into the `to` object.",
        "arguments": [
          {
            "name": "to",
            "description": "The object to merge into.",
            "required": true,
            "type": [
              "object"
            ]
          },
          {
            "name": "from",
            "description": "The object to merge from.",
            "required": true,
            "type": [
              "object"
            ]
          },
          {
            "name": "deep",
            "description": "A deep merge is performed if `true`, otherwise only top-level fields are merged.",
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
          ],
          "rules": [
            "The field from the `from` object is chosen if a key exists in both objects.",
            "Objects are merged recursively if `deep` is specified, a key exists in both objects, and both of those\nfields are also objects."
          ]
        },
        "examples": [
          {
            "title": "Object merge (shallow)",
            "source": "merge(\n    {\n        \"parent1\": {\n            \"child1\": 1,\n            \"child2\": 2\n        },\n        \"parent2\": {\n            \"child3\": 3\n        }\n    },\n    {\n        \"parent1\": {\n            \"child2\": 4,\n            \"child5\": 5\n        }\n    }\n)\n",
            "return": {
              "parent1": {
                "child2": 4,
                "child5": 5
              },
              "parent2": {
                "child3": 3
              }
            }
          },
          {
            "title": "Object merge (deep)",
            "source": "merge(\n    {\n        \"parent1\": {\n            \"child1\": 1,\n            \"child2\": 2\n        },\n        \"parent2\": {\n            \"child3\": 3\n        }\n    },\n    {\n        \"parent1\": {\n            \"child2\": 4,\n            \"child5\": 5\n        }\n    },\n    deep: true\n)\n",
            "return": {
              "parent1": {
                "child1": 1,
                "child2": 4,
                "child5": 5
              },
              "parent2": {
                "child3": 3
              }
            }
          }
        ],
        "pure": true
      }
    }
  }
}