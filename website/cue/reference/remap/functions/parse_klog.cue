{
  "remap": {
    "functions": {
      "parse_klog": {
        "anchor": "parse_klog",
        "name": "parse_klog",
        "category": "Parse",
        "description": "Parses the `value` using the [klog](https://github.com/kubernetes/klog) format used by Kubernetes components.",
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
          "`value` does not match the `klog` format."
        ],
        "examples": [
          {
            "title": "Parse using klog",
            "source": "parse_klog!(\"I0505 17:59:40.692994   28133 klog.go:70] hello from klog\")",
            "return": {
              "file": "klog.go",
              "id": 28133,
              "level": "info",
              "line": 70,
              "message": "hello from klog",
              "timestamp": "2026-05-05T17:59:40.692994Z"
            }
          }
        ],
        "notices": [
          "This function resolves the year for messages. If the current month is January and the\nprovided month is December, it sets the year to the previous year. Otherwise, it sets\nthe year to the current year."
        ],
        "pure": true
      }
    }
  }
}