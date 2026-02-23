{
  "remap": {
    "functions": {
      "parse_glog": {
        "anchor": "parse_glog",
        "name": "parse_glog",
        "category": "Parse",
        "description": "Parses the `value` using the [glog (Google Logging Library)](https://github.com/google/glog) format.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to parse.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "internal_failure_reasons": [
          "`value` does not match the `glog` format."
        ],
        "examples": [
          {
            "title": "Parse using glog",
            "source": "parse_glog!(\"I20210131 14:48:54.411655 15520 main.c++:9] Hello world!\")",
            "return": {
              "file": "main.c++",
              "id": 15520,
              "level": "info",
              "line": 9,
              "message": "Hello world!",
              "timestamp": "2021-01-31T14:48:54.411655Z"
            }
          }
        ],
        "pure": true
      }
    }
  }
}