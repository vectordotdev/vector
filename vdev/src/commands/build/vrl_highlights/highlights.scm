; From belltoy/zed-vrl, commit 2f145fcb8f6ad24d7f2af46c201d349f3dc33538.
; MIT licensed; see LICENSE in this directory.

; Comments
(comment) @comment

; Literals
(null) @constant.builtin
(boolean) @boolean
(integer) @number
(float) @number
(timestamp) @string.special

; Strings
(string) @string
(raw_string) @string
(regex) @string.regexp
(escape_sequence) @string.escape
(raw_string_escape_sequence) @string.escape
(regex_escape_sequence) @string.escape

(string_template
  "{{" @punctuation.special
  "}}" @punctuation.special)

; Generic identifiers must precede specific captures for tree-sitter-highlight.
(ident) @variable

; Functions
(function_call
  function_name: (ident) @function)

; assert / assert_eq raise errors — highlight distinctly
(function_call
  function_name: (ident) @keyword
  (#any-of? @keyword "assert" "assert_eq"))

; Closure parameters
(closure_variables
  (ident) @variable.parameter)

; VRL event/metadata roots (. and %)
(query
  [(event) (metadata)] @variable.builtin)

; Path fields inside a query
(query
  (path
    [
      (field) @property
      (string) @string
      (index) @number
      "." @punctuation.delimiter
    ]))

; _ (noop assign target)
(noop) @variable.builtin

; Keywords
[
  "if"
  "else"
  "return"
  "abort"
] @keyword

; Operators
[
  "="
  "|="
  "=="
  "!="
  ">"
  ">="
  "<"
  "<="
  "+"
  "-"
  "*"
  "/"
  "&&"
  "||"
  "??"
  "|"
  "!"
  "->"
] @operator

; Punctuation
[":" ";" ","] @punctuation.delimiter

[
  "(" ")"
  "[" "]"
  "{" "}"
] @punctuation.bracket

(closure_variables "|" @punctuation.bracket)
