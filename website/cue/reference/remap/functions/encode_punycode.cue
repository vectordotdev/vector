{
  "remap": {
    "functions": {
      "encode_punycode": {
        "anchor": "encode_punycode",
        "name": "encode_punycode",
        "category": "Codec",
        "description": "Encodes a `value` to [punycode](https://en.wikipedia.org/wiki/Punycode). Useful for internationalized domain names ([IDN](https://en.wikipedia.org/wiki/Internationalized_domain_name)). This function assumes that the value passed is meant to be used in IDN context and that it is either a domain name or a part of it.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to encode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "validate",
            "description": "Whether to validate the input string to check if it is a valid domain name.",
            "required": false,
            "type": [
              "boolean"
            ],
            "default": "true"
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "internal_failure_reasons": [
          "`value` can not be encoded to `punycode`"
        ],
        "examples": [
          {
            "title": "Encode an internationalized domain name",
            "source": "encode_punycode!(\"www.café.com\")",
            "return": "www.xn--caf-dma.com"
          },
          {
            "title": "Encode an internationalized domain name with mixed case",
            "source": "encode_punycode!(\"www.CAFé.com\")",
            "return": "www.xn--caf-dma.com"
          },
          {
            "title": "Encode an ASCII only string",
            "source": "encode_punycode!(\"www.cafe.com\")",
            "return": "www.cafe.com"
          },
          {
            "title": "Ignore validation",
            "source": "encode_punycode!(\"xn--8hbb.xn--fiba.xn--8hbf.xn--eib.\", validate: false)",
            "return": "xn--8hbb.xn--fiba.xn--8hbf.xn--eib."
          }
        ],
        "pure": true
      }
    }
  }
}
