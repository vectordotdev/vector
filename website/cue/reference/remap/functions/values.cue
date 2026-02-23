{
  "remap": {
    "functions": {
      "values": {
        "anchor": "values",
        "name": "values",
        "category": "Enumerate",
        "description": "Returns the values from the object passed into the function.",
        "arguments": [
          {
            "name": "value",
            "description": "The object to extract values from.",
            "required": true,
            "type": [
              "object"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ],
          "rules": [
            "Returns an array of all the values."
          ]
        },
        "examples": [
          {
            "title": "Get values from the object",
            "source": "values({\"key1\": \"val1\", \"key2\": \"val2\"})",
            "return": [
              "val1",
              "val2"
            ]
          },
          {
            "title": "Get values from a complex object",
            "source": "values({\"key1\": \"val1\", \"key2\": [1, 2, 3], \"key3\": {\"foo\": \"bar\"}})",
            "return": [
              "val1",
              [
                1,
                2,
                3
              ],
              {
                "foo": "bar"
              }
            ]
          }
        ],
        "pure": true
      }
    }
  }
}