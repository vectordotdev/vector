{
  "remap": {
    "functions": {
      "sieve": {
        "anchor": "sieve",
        "name": "sieve",
        "category": "String",
        "description": "Keeps only matches of `pattern` in `value`.\n\nThis can be used to define patterns that are allowed in the string and\nremove everything else.",
        "arguments": [
          {
            "name": "value",
            "description": "The original string.",
            "required": true,
            "type": [
              "string"
            ]
          },
          {
            "name": "permitted_characters",
            "description": "Keep all matches of this pattern.",
            "required": true,
            "type": [
              "regex"
            ]
          },
          {
            "name": "replace_single",
            "description": "The string to use to replace single rejected characters.",
            "required": false,
            "type": [
              "string"
            ],
            "default": ""
          },
          {
            "name": "replace_repeated",
            "description": "The string to use to replace multiple sequential instances of rejected characters.",
            "required": false,
            "type": [
              "string"
            ],
            "default": ""
          }
        ],
        "return": {
          "types": [
            "string"
          ]
        },
        "examples": [
          {
            "title": "Keep only lowercase letters",
            "source": "sieve(\"vector.dev/lowerUPPER\", permitted_characters: r'[a-z]')",
            "return": "vectordevlower"
          },
          {
            "title": "Sieve with regex",
            "source": "sieve(\"test123%456.فوائد.net.\", r'[a-z0-9.]')",
            "return": "test123456..net."
          },
          {
            "title": "Custom replacements",
            "source": "sieve(\"test123%456.فوائد.net.\", r'[a-z.0-9]', replace_single: \"X\", replace_repeated: \"<REMOVED>\")",
            "return": "test123X456.<REMOVED>.net."
          }
        ],
        "pure": true
      }
    }
  }
}