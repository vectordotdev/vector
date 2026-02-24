{
  "remap": {
    "functions": {
      "length": {
        "anchor": "length",
        "name": "length",
        "category": "Enumerate",
        "description": "Returns the length of the `value`.\n\n* If `value` is an array, returns the number of elements.\n* If `value` is an object, returns the number of top-level keys.\n* If `value` is a string, returns the number of bytes in the string. If\n  you want the number of characters, see `strlen`.",
        "arguments": [
          {
            "name": "value",
            "description": "The array or object.",
            "required": true,
            "type": [
              "string",
              "object",
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ],
          "rules": [
            "If `value` is an array, returns the number of elements.",
            "If `value` is an object, returns the number of top-level keys.",
            "If `value` is a string, returns the number of bytes in the string."
          ]
        },
        "examples": [
          {
            "title": "Length (object)",
            "source": "length({\n    \"portland\": \"Trail Blazers\",\n    \"seattle\": \"Supersonics\"\n})\n",
            "return": 2
          },
          {
            "title": "Length (nested object)",
            "source": "length({\n    \"home\": {\n        \"city\":  \"Portland\",\n        \"state\": \"Oregon\"\n    },\n    \"name\": \"Trail Blazers\",\n    \"mascot\": {\n        \"name\": \"Blaze the Trail Cat\"\n    }\n})\n",
            "return": 3
          },
          {
            "title": "Length (array)",
            "source": "length([\"Trail Blazers\", \"Supersonics\", \"Grizzlies\"])",
            "return": 3
          },
          {
            "title": "Length (string)",
            "source": "length(\"The Planet of the Apes Musical\")",
            "return": 30
          }
        ],
        "pure": true
      }
    }
  }
}
