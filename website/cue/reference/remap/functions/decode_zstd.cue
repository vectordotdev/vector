{
  "remap": {
    "functions": {
      "decode_zstd": {
        "anchor": "decode_zstd",
        "name": "decode_zstd",
        "category": "Codec",
        "description": "Decodes the `value` (a [Zstandard](https://facebook.github.io/zstd) string) into its original string.",
        "arguments": [
          {
            "name": "value",
            "description": "The [Zstandard](https://facebook.github.io/zstd) data to decode.",
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
          "`value` isn't a valid encoded Zstd string."
        ],
        "examples": [
          {
            "title": "Decode Zstd data",
            "source": "decode_zstd!(decode_base64!(\"KLUv/QBY/QEAYsQOFKClbQBedqXsb96EWDax/f/F/z+gNU4ZTInaUeAj82KqPFjUzKqhcfDqAIsLvAsnY1bI/N2mHzDixRQA\"))",
            "return": "you_have_successfully_decoded_me.congratulations.you_are_breathtaking."
          }
        ],
        "pure": true
      }
    }
  }
}
