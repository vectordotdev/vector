{
  "remap": {
    "functions": {
      "shannon_entropy": {
        "anchor": "shannon_entropy",
        "name": "shannon_entropy",
        "category": "String",
        "description": "Generates [Shannon entropy](https://en.wikipedia.org/wiki/Entropy_(information_theory)) from given string. It can generate it based on string bytes, codepoints, or graphemes.",
        "arguments": [
          {
            "name": "value",
            "description": "The input string.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "segmentation",
            "description": "Defines how to split the string to calculate entropy, based on occurrences of\nsegments.\n\nByte segmentation is the fastest, but it might give undesired results when handling\nUTF-8 strings, while grapheme segmentation is the slowest, but most correct in these\ncases.",
            "required": false,
            "type": [
              "string"
            ],
            "enum": {
              "grapheme": "Considers graphemes when calculating entropy",
              "byte": "Considers individual bytes when calculating entropy",
              "codepoint": "Considers codepoints when calculating entropy"
            },
            "default": "byte"
          }
        ],
        "return": {
          "types": [
            "float"
          ]
        },
        "examples": [
          {
            "title": "Simple byte segmentation example",
            "source": "floor(shannon_entropy(\"vector.dev\"), precision: 4)",
            "return": 2.9219
          },
          {
            "title": "UTF-8 string with bytes segmentation",
            "source": "floor(shannon_entropy(\"test123%456.فوائد.net.\"), precision: 4)",
            "return": 4.0784
          },
          {
            "title": "UTF-8 string with grapheme segmentation",
            "source": "floor(shannon_entropy(\"test123%456.فوائد.net.\", segmentation: \"grapheme\"), precision: 4)",
            "return": 3.9362
          },
          {
            "title": "UTF-8 emoji (7 Unicode scalar values) with grapheme segmentation",
            "source": "shannon_entropy(\"👨‍👩‍👧‍👦\", segmentation: \"grapheme\")",
            "return": 0.0
          }
        ],
        "pure": true
      }
    }
  }
}