{
  "remap": {
    "functions": {
      "split_path": {
        "anchor": "split_path",
        "name": "split_path",
        "category": "String",
        "description": "Splits the given `path` into its constituent components, returning an array of strings. Each component represents a part of the file system path hierarchy.",
        "arguments": [
          {
            "name": "value",
            "description": "The path to split into components.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "array"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid string."
        ],
        "examples": [
          {
            "title": "Split path with trailing slash",
            "source": "split_path(\"/home/user/\")",
            "return": [
              "/",
              "home",
              "user"
            ]
          },
          {
            "title": "Split path from file path",
            "source": "split_path(\"/home/user\")",
            "return": [
              "/",
              "home",
              "user"
            ]
          },
          {
            "title": "Split path from root",
            "source": "split_path(\"/\")",
            "return": [
              "/"
            ]
          },
          {
            "title": "Empty path returns empty array",
            "source": "split_path(\"\")",
            "return": []
          }
        ],
        "pure": true
      }
    }
  }
}