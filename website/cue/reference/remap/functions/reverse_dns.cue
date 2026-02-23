{
  "remap": {
    "functions": {
      "reverse_dns": {
        "anchor": "reverse_dns",
        "name": "reverse_dns",
        "category": "System",
        "description": "Performs a reverse DNS lookup on the provided IP address to retrieve the associated hostname.",
        "arguments": [
          {
            "name": "value",
            "description": "The IP address (IPv4 or IPv6) to perform the reverse DNS lookup on.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Example",
            "source": "reverse_dns!(\"127.0.0.1\")",
            "return": "localhost"
          }
        ],
        "pure": true
      }
    }
  }
}