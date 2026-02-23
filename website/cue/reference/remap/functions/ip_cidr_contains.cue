{
  "remap": {
    "functions": {
      "ip_cidr_contains": {
        "anchor": "ip_cidr_contains",
        "name": "ip_cidr_contains",
        "category": "IP",
        "description": "Determines whether the `ip` is contained in the block referenced by the `cidr`.",
        "arguments": [
          {
            "name": "cidr",
            "description": "The CIDR mask (v4 or v6).",
            "required": true,
            "type": [
              "string",
              "array"
            ]
          },
          {
            "name": "value",
            "description": "The IP address (v4 or v6).",
            "required": true,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "internal_failure_reasons": [
          "`cidr` is not a valid CIDR.",
          "`ip` is not a valid IP address."
        ],
        "examples": [
          {
            "title": "IPv4 contains CIDR",
            "source": "ip_cidr_contains!(\"192.168.0.0/16\", \"192.168.10.32\")",
            "return": true
          },
          {
            "title": "IPv4 is private",
            "source": "ip_cidr_contains!([\"10.0.0.0/8\", \"172.16.0.0/12\", \"192.168.0.0/16\"], \"192.168.10.32\")",
            "return": true
          },
          {
            "title": "IPv6 contains CIDR",
            "source": "ip_cidr_contains!(\"2001:4f8:4:ba::/64\", \"2001:4f8:4:ba:2e0:81ff:fe22:d1f1\")",
            "return": true
          },
          {
            "title": "Not in range",
            "source": "ip_cidr_contains!(\"192.168.0.0/24\", \"192.168.10.32\")",
            "return": false
          },
          {
            "title": "Invalid address",
            "source": "ip_cidr_contains!(\"192.168.0.0/24\", \"INVALID\")",
            "raises": "function call error for \"ip_cidr_contains\" at (0:46): unable to parse IP address: invalid IP address syntax"
          }
        ],
        "pure": true
      }
    }
  }
}