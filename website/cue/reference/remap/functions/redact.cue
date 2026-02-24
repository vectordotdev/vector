{
  "remap": {
    "functions": {
      "redact": {
        "anchor": "redact",
        "name": "redact",
        "category": "String",
        "description": "Redact sensitive data in `value` such as:\n\n- [US social security card numbers](https://www.ssa.gov/history/ssn/geocard.html)\n- Other forms of personally identifiable information with custom patterns\n\nThis can help achieve compliance by ensuring sensitive data does not leave your network.",
        "arguments": [
          {
            "name": "value",
            "description": "The value to redact sensitive data from.\n\nThe function's behavior depends on `value`'s type:\n\n- For strings, the sensitive data is redacted and a new string is returned.\n- For arrays, the sensitive data is redacted in each string element.\n- For objects, the sensitive data in each string value is masked, but the keys are not masked.\n\nFor arrays and objects, the function recurses into any nested arrays or objects. Any non-string elements are\nskipped.\n\nRedacted text is replaced with `[REDACTED]`.",
            "required": true,
            "type": [
              "string",
              "object",
              "array"
            ]
          },
          {
            "name": "filters",
            "description": "List of filters applied to `value`.\n\nEach filter can be specified in the following ways:\n\n- As a regular expression, which is used to redact text that match it.\n- As an object with a `type` key that corresponds to a named filter and additional keys for customizing that filter.\n- As a named filter, if it has no required parameters.\n\nNamed filters can be a:\n\n- `pattern`: Redacts text matching any regular expressions specified in the `patterns`\n\tkey, which is required. This is the expanded version of just passing a regular expression as a filter.\n- `us_social_security_number`: Redacts US social security card numbers.\n\nSee examples for more details.\n\nThis parameter must be a static expression so that the argument can be validated at compile-time\nto avoid runtime errors. You cannot use variables or other dynamic expressions with it.",
            "required": true,
            "type": [
              "array"
            ]
          },
          {
            "name": "redactor",
            "description": "Specifies what to replace the redacted strings with.\n\nIt is given as an object with a \"type\" key specifying the type of redactor to use\nand additional keys depending on the type. The following types are supported:\n\n- `full`: The default. Replace with the string \"[REDACTED]\".\n- `text`: Replace with a custom string. The `replacement` key is required, and must\n  contain the string that is used as a replacement.\n- `sha2`: Hash the redacted text with SHA-2 as with [`sha2`](https://en.wikipedia.org/wiki/SHA-2). Supports two optional parameters:\n\t- `variant`: The variant of the algorithm to use. Defaults to SHA-512/256.\n\t- `encoding`: How to encode the hash as text. Can be base16 or base64.\n\t\tDefaults to base64.\n- `sha3`: Hash the redacted text with SHA-3 as with [`sha3`](https://en.wikipedia.org/wiki/SHA-3). Supports two optional parameters:\n\t- `variant`: The variant of the algorithm to use. Defaults to SHA3-512.\n\t- `encoding`: How to encode the hash as text. Can be base16 or base64.\n\t\tDefaults to base64.\n\n\nAs a convenience you can use a string as a shorthand for common redactor patterns:\n\n- `\"full\"` is equivalent to `{\"type\": \"full\"}`\n- `\"sha2\"` is equivalent to `{\"type\": \"sha2\", \"variant\": \"SHA-512/256\", \"encoding\": \"base64\"}`\n- `\"sha3\"` is equivalent to `{\"type\": \"sha3\", \"variant\": \"SHA3-512\", \"encoding\": \"base64\"}`\n\nThis parameter must be a static expression so that the argument can be validated at compile-time\nto avoid runtime errors. You cannot use variables or other dynamic expressions with it.",
            "required": false,
            "type": [
              "string",
              "object"
            ]
          }
        ],
        "return": {
          "types": [
            "string",
            "object",
            "array"
          ]
        },
        "examples": [
          {
            "title": "Replace text using a regex",
            "source": "redact(\"my id is 123456\", filters: [r'\\d+'])",
            "return": "my id is [REDACTED]"
          },
          {
            "title": "Replace us social security numbers in any field",
            "source": "redact({ \"name\": \"John Doe\", \"ssn\": \"123-12-1234\"}, filters: [\"us_social_security_number\"])",
            "return": {
              "name": "John Doe",
              "ssn": "[REDACTED]"
            }
          },
          {
            "title": "Replace with custom text",
            "source": "redact(\"my id is 123456\", filters: [r'\\d+'], redactor: {\"type\": \"text\", \"replacement\": \"***\"})",
            "return": "my id is ***"
          },
          {
            "title": "Replace with SHA-2 hash",
            "source": "redact(\"my id is 123456\", filters: [r'\\d+'], redactor: \"sha2\")",
            "return": "my id is GEtTedW1p6tC094dDKH+3B8P+xSnZz69AmpjaXRd63I="
          },
          {
            "title": "Replace with SHA-3 hash",
            "source": "redact(\"my id is 123456\", filters: [r'\\d+'], redactor: \"sha3\")",
            "return": "my id is ZNCdmTDI7PeeUTFnpYjLdUObdizo+bIupZdl8yqnTKGdLx6X3JIqPUlUWUoFBikX+yTR+OcvLtAqWO11NPlNJw=="
          },
          {
            "title": "Replace with SHA-256 hash using hex encoding",
            "source": "redact(\"my id is 123456\", filters: [r'\\d+'], redactor: {\"type\": \"sha2\", \"variant\": \"SHA-256\", \"encoding\": \"base16\"})",
            "return": "my id is 8d969eef6ecad3c29a3a629280e686cf0c3f5d5a86aff3ca12020c923adc6c92"
          }
        ],
        "pure": true
      }
    }
  }
}
