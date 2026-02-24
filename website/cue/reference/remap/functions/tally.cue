{
  "remap": {
    "functions": {
      "tally": {
        "anchor": "tally",
        "name": "tally",
        "category": "Enumerate",
        "description": "Counts the occurrences of each string value in the provided array and returns an object with the counts.",
        "arguments": [
          {
            "name": "value",
            "description": "The array of strings to count occurrences for.",
            "required": true,
            "type": [
              "array"
            ]
          }
        ],
        "return": {
          "types": [
            "object"
          ]
        },
        "examples": [
          {
            "title": "tally",
            "source": "tally!([\"foo\", \"bar\", \"foo\", \"baz\"])",
            "return": {
              "foo": 2,
              "bar": 1,
              "baz": 1
            }
          }
        ],
        "pure": true
      }
    }
  }
}
