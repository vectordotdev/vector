{
  "remap": {
    "functions": {
      "keys": {
        "anchor": "keys",
        "name": "keys",
        "category": "Enumerate",
        "description": "Returns the keys from the object passed into the function.",
        "arguments": [
          {
            "name": "value",
            "description": "The object to extract keys from.",
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
            "Returns an array of all the keys"
          ]
        },
        "examples": [
          {
            "title": "Get keys from the object",
            "source": "keys({\n    \"key1\": \"val1\",\n    \"key2\": \"val2\"\n})\n",
            "return": [
              "key1",
              "key2"
            ]
          }
        ],
        "pure": true
      }
    }
  }
}