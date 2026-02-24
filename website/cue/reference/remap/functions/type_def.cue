{
  "remap": {
    "functions": {
      "type_def": {
        "anchor": "type_def",
        "name": "type_def",
        "category": "Type",
        "description": "Returns the type definition of an expression at runtime.\n\nThis is a debug function that is *UNSTABLE*. Behavior is *NOT* guaranteed even though it is technically usable.",
        "arguments": [
          {
            "name": "value",
            "description": "The expression to get the type definition for.",
            "required": true,
            "type": [
              "any"
            ]
          }
        ],
        "return": {
          "types": [
            "any"
          ]
        },
        "examples": [
          {
            "title": "return type definition",
            "source": "type_def(42)",
            "return": {
              "integer": true
            }
          }
        ],
        "pure": true
      }
    }
  }
}
