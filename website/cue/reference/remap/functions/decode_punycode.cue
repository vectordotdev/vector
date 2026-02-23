{
  "remap": {
    "functions": {
      "decode_punycode": {
        "anchor": "decode_punycode",
        "name": "decode_punycode",
        "category": "Codec",
        "description": "Decodes a [punycode](https://en.wikipedia.org/wiki/Punycode) encoded `value`, such as an internationalized domain name ([IDN](https://en.wikipedia.org/wiki/Internationalized_domain_name)). This function assumes that the value passed is meant to be used in IDN context and that it is either a domain name or a part of it.",
        "arguments": [
          {
            "name": "value",
            "description": "The string to decode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "validate",
            "description": "If enabled, checks if the input string is a valid domain name.",
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
          "`value` is not valid `punycode`"
        ],
        "examples": [
          {
            "title": "Decode a punycode encoded internationalized domain name",
            "source": "decode_punycode!(\"www.xn--caf-dma.com\")",
            "return": "www.café.com"
          },
          {
            "title": "Decode an ASCII only string",
            "source": "decode_punycode!(\"www.cafe.com\")",
            "return": "www.cafe.com"
          },
          {
            "title": "Ignore validation",
            "source": "decode_punycode!(\"xn--8hbb.xn--fiba.xn--8hbf.xn--eib.\", validate: false)",
            "return": "١٠.٦٦.٣٠.٥."
          }
        ],
        "pure": true
      }
    }
  }
}