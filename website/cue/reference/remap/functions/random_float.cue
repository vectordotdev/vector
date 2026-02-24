{
  "remap": {
    "functions": {
      "random_float": {
        "anchor": "random_float",
        "name": "random_float",
        "category": "Random",
        "description": "Returns a random float between [min, max).",
        "arguments": [
          {
            "name": "min",
            "description": "Minimum value (inclusive).",
            "required": true,
            "type": [
              "float"
            ]
          },
          {
            "name": "max",
            "description": "Maximum value (exclusive).",
            "required": true,
            "type": [
              "float"
            ]
          }
        ],
        "return": {
          "types": [
            "float"
          ]
        },
        "internal_failure_reasons": [
          "`max` is not greater than `min`."
        ],
        "examples": [
          {
            "title": "Random float from 0.0 to 10.0, not including 10.0",
            "source": "f = random_float(0.0, 10.0)\nf >= 0 && f < 10\n",
            "return": true
          }
        ],
        "pure": true
      }
    }
  }
}
