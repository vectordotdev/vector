{
  "remap": {
    "functions": {
      "ip_aton": {
        "anchor": "ip_aton",
        "name": "ip_aton",
        "category": "IP",
        "description": "Converts IPv4 address in numbers-and-dots notation into network-order\nbytes represented as an integer.\n\nThis behavior mimics [inet_aton](https://linux.die.net/man/3/inet_aton).",
        "arguments": [
          {
            "name": "value",
            "description": "The IP address to convert to binary.",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "integer"
          ]
        },
        "internal_failure_reasons": [
          "`value` is not a valid IPv4 address."
        ],
        "examples": [
          {
            "title": "IPv4 to integer",
            "source": "ip_aton!(\"1.2.3.4\")",
            "return": 16909060
          }
        ],
        "pure": true
      }
    }
  }
}
