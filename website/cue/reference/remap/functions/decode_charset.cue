{
  "remap": {
    "functions": {
      "decode_charset": {
        "anchor": "decode_charset",
        "name": "decode_charset",
        "category": "Codec",
        "description": "Decodes the `value` (a non-UTF8 string) to a UTF8 string using the specified\n[character set](https://encoding.spec.whatwg.org/#names-and-labels).",
        "arguments": [
          {
            "name": "value",
            "description": "The non-UTF8 string to decode.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "from_charset",
            "description": "The [character set](https://encoding.spec.whatwg.org/#names-and-labels) to use when decoding the data.",
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
          "`from_charset` isn't a valid [character set](https://encoding.spec.whatwg.org/#names-and-labels)."
        ],
        "examples": [
          {
            "title": "Decode EUC-KR string",
            "source": "decode_charset!(decode_base64!(\"vsiz58fPvLy/5A==\"), \"euc-kr\")",
            "return": "안녕하세요"
          },
          {
            "title": "Decode EUC-JP string",
            "source": "decode_charset!(decode_base64!(\"pLOk86TLpMGkzw==\"), \"euc-jp\")",
            "return": "こんにちは"
          },
          {
            "title": "Decode GB2312 string",
            "source": "decode_charset!(decode_base64!(\"xOO6ww==\"), \"gb2312\")",
            "return": "你好"
          }
        ],
        "pure": true
      }
    }
  }
}
