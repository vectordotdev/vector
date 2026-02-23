{
  "remap": {
    "functions": {
      "ip_subnet": {
        "anchor": "ip_subnet",
        "name": "ip_subnet",
        "category": "IP",
        "description": "Extracts the subnet address from the `ip` using the supplied `subnet`.",
        "arguments": [
          {
            "name": "value",
            "description": "The IP address (v4 or v6).",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "subnet",
            "description": "The subnet to extract from the IP address. This can be either a prefix length like `/8` or a net mask\nlike `255.255.0.0`. The net mask can be either an IPv4 or IPv6 address.",
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
        "internal_failure_reasons": [
          "`ip` is not a valid IP address.",
          "`subnet` is not a valid subnet."
        ],
        "examples": [
          {
            "title": "IPv4 subnet",
            "source": "ip_subnet!(\"192.168.10.32\", \"255.255.255.0\")",
            "return": "192.168.10.0"
          },
          {
            "title": "IPv6 subnet",
            "source": "ip_subnet!(\"2404:6800:4003:c02::64\", \"/32\")",
            "return": "2404:6800::"
          },
          {
            "title": "Subnet /1",
            "source": "ip_subnet!(\"192.168.0.1\", \"/1\")",
            "return": "128.0.0.0"
          }
        ],
        "notices": [
          "Works with both IPv4 and IPv6 addresses. The IP version for the mask must be the same as\nthe supplied address."
        ],
        "pure": true
      }
    }
  }
}