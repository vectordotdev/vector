{
  "remap": {
    "functions": {
      "assert": {
        "anchor": "assert",
        "name": "assert",
        "category": "Debug",
        "description": "Asserts the `condition`, which must be a Boolean expression. The program is aborted with `message` if the condition evaluates to `false`.",
        "arguments": [
          {
            "name": "condition",
            "description": "The condition to check.",
            "required": true,
            "type": [
              "boolean"
            ]
          },
          {
            "name": "message",
            "description": "An optional custom error message. If the equality assertion fails, `message` is\nappended to the default message prefix. See the [examples](#assert-examples) below\nfor a fully formed log message sample.",
            "required": false,
            "type": [
              "string"
            ]
          }
        ],
        "return": {
          "types": [
            "boolean"
          ]
        },
        "internal_failure_reasons": [
          "`condition` evaluates to `false`."
        ],
        "examples": [
          {
            "title": "Assertion (true) - with message",
            "source": "assert!(\"foo\" == \"foo\", message: \"\\\"foo\\\" must be \\\"foo\\\"!\")",
            "return": true
          },
          {
            "title": "Assertion (false) - with message",
            "source": "assert!(\"foo\" == \"bar\", message: \"\\\"foo\\\" must be \\\"foo\\\"!\")",
            "raises": "function call error for \"assert\" at (0:60): \"foo\" must be \"foo\"!"
          },
          {
            "title": "Assertion (false) - simple",
            "source": "assert!(false)",
            "raises": "function call error for \"assert\" at (0:14): assertion failed"
          }
        ],
        "notices": [
          "The `assert` function should be used in a standalone fashion and only when you want\nto abort the program. You should avoid it in logical expressions and other situations\nin which you want the program to continue if the condition evaluates to `false`."
        ],
        "pure": false
      }
    }
  }
}
