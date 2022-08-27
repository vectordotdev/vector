
let whole_number = /[0-9_]+/;
let local_identifier = /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/;
let query_field_identifier = /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/;
let quoted_field_identifier = /(\\"|[^"])+/;

function expr_list($, expr) {
  return seq(
    expr,
    repeat(seq(
      $.expr_end_with_leading_whitespace,
      optional($.ws),
      expr,
    )),
    optional($.expr_end_with_leading_whitespace),
  );
}


module.exports = grammar({
  name: 'vrl',

  extras: $ => [],
  inline: $ => [
    $.expr_end_with_leading_whitespace,
    $.expr_end,
    $.ws,
    $.comment,
  ],

  conflicts: $ => [
    [$.exprs],
    [$.if_statement],
    [$.predicate_exprs],
    [$.assignment_expr],
    [$.predicate_exprs, $.expr],

    [$.literal, $.object_entry],
    [$.predicate_exprs, $.group],
    [$.arithmetic_expr_class, $.assignment_target],
    [$.arithmetic_expr_class, $.assignment_expr_class],
    [$.arithmetic_expr_class, $.predicate],
    [$.arithmetic_expr_class],
    [$.error_coalesce_expr, $.assignment_expr_class],
    [$.error_coalesce_expr, $.predicate],
    [$.error_coalesce_expr]
  ],

  rules: {

    program: $ => seq(optional($.ws), $.exprs, optional($.ws)),

    // horizontal whitespace
    _horizontal_ws: $ => repeat1($._single_horizontal_ws),

    _single_horizontal_ws: $ => /[ \r\t]/,

    _newline: $ => "\n",

    comment: $ => seq(
      "#",
      /[^\n]*/
    ),

    ws: $ => repeat1(choice(
      $.comment,
      $._single_horizontal_ws,
      $._newline,
    )),

    expr_end: $ => choice(";", $._newline),

    expr_end_with_leading_whitespace: $ => seq(
      optional($._horizontal_ws),
      $.expr_end
    ),

    exprs: $ => expr_list($, $.expr),

    predicate_exprs: $ => expr_list($, alias($.assignment_expr_class, $.expr)),

    expr: $ => choice(
      $.if_statement,
      $.abort,
      alias($.assignment_expr_class, "hidden"),
    ),

    arithmetic_expr_class: $ => choice(
      $.literal,
      $.query,
      $.container,
      $.error_coalesce_expr,
//      $.or_expr,
    ),

    error_coalesce_expr: $ => seq(
      alias($.arithmetic_expr_class, $.expr),
      optional($.ws),
      "??",
      optional($.ws),
      alias($.arithmetic_expr_class, $.expr),
    ),

    or_expr: $ => prec.left(2, seq(
     alias($.arithmetic_expr_class, $.expr),
     optional($.ws),
     "||",
     optional($.ws),
     alias($.arithmetic_expr_class, $.expr),
    )),

    assignment_expr_class: $ => choice(
      $.assignment_expr,
      alias($.arithmetic_expr_class, "hidden"),
    ),

    assignment_target: $ => choice(
      $.assignment_target_noop,
      $.query,
    ),

    assignment_target_noop: $ => "_",

    assignment_op: $ => choice("=", "|="),

    assignment_expr: $ => seq(
      $.assignment_target,
      optional(seq(
        optional($._horizontal_ws),
        ",",
        optional($._horizontal_ws),
        $.assignment_target,
      )),
      optional($.ws),
      $.assignment_op,
      optional($.ws),
      $.expr
    ),

    abort: $ => prec.right(0, seq(
      "abort",
      optional($._horizontal_ws),
      optional($.expr),
    )),

    if_statement: $ => seq(
      "if",
      $._horizontal_ws,
      $.predicate,
      optional($.ws),
      $.block,
      repeat(seq(
        optional($.ws),
        "else if",
        $._horizontal_ws,
        $.predicate,
        optional($.ws),
        $.block,
      )),
      optional(seq(
        optional($.ws),
        "else",
        optional($.ws),
        $.block,
      ))
    ),

    predicate: $ => choice(
      alias($.arithmetic_expr_class, $.expr),
      seq(
        "(",
        optional($.ws),
        alias($.predicate_exprs, $.exprs),
        optional($.ws),
        ")"
      )
    ),

    literal: $ => choice(
      $.float,
      $.integer,
      $.string,
      $.raw_string,
      $.regex,
      $.boolean,
      $.null,
      $.timestamp,
    ),

    container: $ => choice(
      $.group,
      $.block,
      $.array,
      $.object,
    ),

    _brace_open: $ => "{",
    _brace_close: $ => "}",

    object: $ => seq(
      $._brace_open,
      optional($.ws),
      optional(seq(
        $.object_entry,
        repeat(seq(
          optional($.ws),
          ",",
          optional($.ws),
          $.object_entry,
        )),
        optional($.ws),
        optional(seq(",", optional($.ws))),
      )),
      $._brace_close,
    ),

    object_entry: $ => seq(
      $.string,
      optional($.ws),
      ":",
      optional($.ws),
      $.expr,
    ),

    array: $ => seq(
      "[",
      optional($.ws),
      optional(seq(
        $.expr,
        repeat(seq(
          optional($.ws),
          ",",
          optional($.ws),
          $.expr,
        )),
        optional($.ws),
        optional(seq(",", optional($.ws))),
      )),
      "]",
    ),

    group: $ => seq(
      "(",
      optional($.ws),
      $.expr,
      optional($.ws),
      ")",
    ),

    block: $ => seq(
      $._brace_open,
      optional($.ws),
      $.exprs,
      optional($.ws),
      $._brace_close,
    ),

    null: $ => "null",

    string: $ => seq(
      $._single_quote,
      repeat(choice(
        /[^"\\]/,
        seq("\\", /[n'"\\nrt{}]/)
      )),
      $._single_quote,
    ),

    boolean: $ => choice("true", "false"),

    raw_string: $ => seq("s", $.raw),

    regex: $ => seq("r", $.raw),

    timestamp: $ => seq("t", $.raw),

    raw: $ => seq(
      "'",
      repeat(choice(
        /\\./,
        /[^\\]/
      )),
      "'",
    ),

    _integer: $ => seq(optional("-"), $._whole_number),

    integer: $ => $._integer,

    whole_number: $ => whole_number,
    _whole_number: $ => whole_number,

    float: $ => seq(
      $._integer,
      ".",
      $._whole_number,
    ),

    query: $ => choice(
      $.local_query,
      $.target_query,
      $.container_query,
    ),

    container_query: $ => seq(
      $.container,
      $.query_segments
    ),

    local_query: $ => seq(
      $.local_variable,
      optional($.query_segments)
    ),

    local_variable: $ => local_identifier,

    query_segments: $ => repeat1($._query_segment),

    query_segments_without_dot: $ => seq(
      $._query_segment_without_dot,
      repeat($._query_segment)
    ),

    _query_segment: $ => choice(
      seq(".", $.query_field),
      $.query_index,
      seq(".", $.query_coalesce),
    ),

    _query_segment_without_dot: $ => choice(
      $.query_field,
      $.query_index,
      $.query_coalesce,
    ),

    _single_quote: $ => "\"",

    quoted_field: $ => quoted_field_identifier,

    query_index: $ => seq(
      "[",
      optional($.ws),
      $.integer,
      optional($.ws),
      "]"
    ),

    query_field: $ => choice(
      query_field_identifier,
      seq(
        $._single_quote,
        $.quoted_field,
        $._single_quote,
      )
    ),

    query_coalesce: $ => seq(
      "(",
      optional($.ws),
      $.query_field,
      repeat1(seq(
        optional($.ws),
        "|",
        optional($.ws),
        $.query_field,
      )),
      optional($.ws),
      ")"
    ),

    target_query: $ => seq(
      $.target_query_prefix,
      optional(alias($.query_segments_without_dot, $.query_segments))
    ),

    target_query_prefix: $ => choice(
      ".",
      "%",
    )

  }
});
