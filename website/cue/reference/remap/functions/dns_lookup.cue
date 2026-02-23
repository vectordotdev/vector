{
  "remap": {
    "functions": {
      "dns_lookup": {
        "anchor": "dns_lookup",
        "name": "dns_lookup",
        "category": "System",
        "description": "Performs a DNS lookup on the provided domain name. This function performs network calls and blocks on each request until a response is received. It is not recommended for frequent or performance-critical workflows.",
        "arguments": [
          {
            "name": "value",
            "description": "The domain name to query.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "qtype",
            "description": "The DNS record type to query (e.g., A, AAAA, MX, TXT). Defaults to A.",
            "required": false,
            "type": [
              "string"
            ],
            "default": "A"
          },
          {
            "name": "class",
            "description": "The DNS query class. Defaults to IN (Internet).",
            "required": false,
            "type": [
              "string"
            ],
            "default": "IN"
          },
          {
            "name": "options",
            "description": "DNS resolver options. Supported fields: servers (array of nameserver addresses), timeout (seconds), attempts (number of retry attempts), ndots, aa_only, tcp, recurse, rotate.",
            "required": false,
            "type": [
              "object"
            ],
            "default": "{  }"
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "examples": [
          {
            "title": "Basic lookup",
            "source": "res = dns_lookup!(\"dns.google\")\n# reset non-static ttl so result is static\nres.answers = map_values(res.answers) -> |value| {\n  value.ttl = 600\n  value\n}\n# remove extra responses for example\nres.answers = filter(res.answers) -> |_, value| {\n    value.rData == \"8.8.8.8\"\n}\n# remove class since this is also dynamic\nres.additional = map_values(res.additional) -> |value| {\n    del(value.class)\n    value\n}\nres\n",
            "return": {
              "additional": [
                {
                  "domainName": "",
                  "rData": "OPT ...",
                  "recordType": "OPT",
                  "recordTypeId": 41,
                  "ttl": 0
                }
              ],
              "answers": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "rData": "8.8.8.8",
                  "recordType": "A",
                  "recordTypeId": 1,
                  "ttl": 600
                }
              ],
              "authority": [],
              "fullRcode": 0,
              "header": {
                "aa": false,
                "ad": false,
                "anCount": 2,
                "arCount": 1,
                "cd": false,
                "nsCount": 0,
                "opcode": 0,
                "qdCount": 1,
                "qr": true,
                "ra": true,
                "rcode": 0,
                "rd": true,
                "tc": false
              },
              "question": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "questionType": "A",
                  "questionTypeId": 1
                }
              ],
              "rcodeName": "NOERROR"
            }
          },
          {
            "title": "Custom class and qtype",
            "source": "res = dns_lookup!(\"dns.google\", class: \"IN\", qtype: \"A\")\n# reset non-static ttl so result is static\nres.answers = map_values(res.answers) -> |value| {\n  value.ttl = 600\n  value\n}\n# remove extra responses for example\nres.answers = filter(res.answers) -> |_, value| {\n    value.rData == \"8.8.8.8\"\n}\n# remove class since this is also dynamic\nres.additional = map_values(res.additional) -> |value| {\n    del(value.class)\n    value\n}\nres\n",
            "return": {
              "additional": [
                {
                  "domainName": "",
                  "rData": "OPT ...",
                  "recordType": "OPT",
                  "recordTypeId": 41,
                  "ttl": 0
                }
              ],
              "answers": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "rData": "8.8.8.8",
                  "recordType": "A",
                  "recordTypeId": 1,
                  "ttl": 600
                }
              ],
              "authority": [],
              "fullRcode": 0,
              "header": {
                "aa": false,
                "ad": false,
                "anCount": 2,
                "arCount": 1,
                "cd": false,
                "nsCount": 0,
                "opcode": 0,
                "qdCount": 1,
                "qr": true,
                "ra": true,
                "rcode": 0,
                "rd": true,
                "tc": false
              },
              "question": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "questionType": "A",
                  "questionTypeId": 1
                }
              ],
              "rcodeName": "NOERROR"
            }
          },
          {
            "title": "Custom options",
            "source": "res = dns_lookup!(\"dns.google\", options: {\"timeout\": 30, \"attempts\": 5})\nres.answers = map_values(res.answers) -> |value| {\n  value.ttl = 600\n  value\n}\n# remove extra responses for example\nres.answers = filter(res.answers) -> |_, value| {\n    value.rData == \"8.8.8.8\"\n}\n# remove class since this is also dynamic\nres.additional = map_values(res.additional) -> |value| {\n    del(value.class)\n    value\n}\nres\n",
            "return": {
              "additional": [
                {
                  "domainName": "",
                  "rData": "OPT ...",
                  "recordType": "OPT",
                  "recordTypeId": 41,
                  "ttl": 0
                }
              ],
              "answers": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "rData": "8.8.8.8",
                  "recordType": "A",
                  "recordTypeId": 1,
                  "ttl": 600
                }
              ],
              "authority": [],
              "fullRcode": 0,
              "header": {
                "aa": false,
                "ad": false,
                "anCount": 2,
                "arCount": 1,
                "cd": false,
                "nsCount": 0,
                "opcode": 0,
                "qdCount": 1,
                "qr": true,
                "ra": true,
                "rcode": 0,
                "rd": true,
                "tc": false
              },
              "question": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "questionType": "A",
                  "questionTypeId": 1
                }
              ],
              "rcodeName": "NOERROR"
            }
          },
          {
            "title": "Custom server",
            "source": "res = dns_lookup!(\"dns.google\", options: {\"servers\": [\"dns.quad9.net\"]})\nres.answers = map_values(res.answers) -> |value| {\n  value.ttl = 600\n  value\n}\n# remove extra responses for example\nres.answers = filter(res.answers) -> |_, value| {\n    value.rData == \"8.8.8.8\"\n}\n# remove class since this is also dynamic\nres.additional = map_values(res.additional) -> |value| {\n    del(value.class)\n    value\n}\nres\n",
            "return": {
              "additional": [
                {
                  "domainName": "",
                  "rData": "OPT ...",
                  "recordType": "OPT",
                  "recordTypeId": 41,
                  "ttl": 0
                }
              ],
              "answers": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "rData": "8.8.8.8",
                  "recordType": "A",
                  "recordTypeId": 1,
                  "ttl": 600
                }
              ],
              "authority": [],
              "fullRcode": 0,
              "header": {
                "aa": false,
                "ad": false,
                "anCount": 2,
                "arCount": 1,
                "cd": false,
                "nsCount": 0,
                "opcode": 0,
                "qdCount": 1,
                "qr": true,
                "ra": true,
                "rcode": 0,
                "rd": true,
                "tc": false
              },
              "question": [
                {
                  "class": "IN",
                  "domainName": "dns.google",
                  "questionType": "A",
                  "questionTypeId": 1
                }
              ],
              "rcodeName": "NOERROR"
            }
          }
        ],
        "pure": true
      }
    }
  }
}