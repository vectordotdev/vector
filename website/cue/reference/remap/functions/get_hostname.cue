{
  "remap": {
    "functions": {
      "get_hostname": {
        "anchor": "get_hostname",
        "name": "get_hostname",
        "category": "System",
        "description": "Returns the local system's hostname.",
        "arguments": [],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "Internal hostname resolution failed."
        ],
        "examples": [
          {
            "title": "Get hostname",
            "source": "get_hostname!()",
            "return": "my-hostname"
          }
        ],
        "pure": true
      }
    }
  }
}
