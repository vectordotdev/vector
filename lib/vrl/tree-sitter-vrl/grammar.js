
let whole_number = /[0-9_]+/;
let local_identifier = /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/;
let query_field_identifier = /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/;
let quoted_field_identifier = /(\\"|[^"])+/;

module.exports = grammar({
  name: 'vrl',

  extras: $ => [],
  inline: $ => [
    $.expr_end_with_leading_whitespace,
    $.expr_end,
    $.ws,
//    $.comment,
  ],

  conflicts: $ => [
    [$.exprs],
    [$.literal, $.object_entry],
  ],

  rules: {

    program: $ => seq(optional($.ws), $.exprs, optional($.ws)),

    // horizontal whitespace
    _horizontal_ws: $ => repeat1($._single_horizontal_ws),

    _single_horizontal_ws: $ => /[ \r\t]/,

    _newline: $ => "\n",

    comment: $ => seq(
      "#",
      /[^\n]*/,
      $._newline,
    ),

    ws: $ => repeat1(choice(
      $.comment,
      $._single_horizontal_ws,
      $._newline,
    )),

    expr_end: $ => choice(";", $._newline, $.comment),

    expr_end_with_leading_whitespace: $ => seq(
      optional($._horizontal_ws),
      $.expr_end
    ),

    exprs: $ => seq(
      $.expr,
      repeat(seq(
        $.expr_end_with_leading_whitespace,
        optional($.ws),
        $.expr,
      )),
      optional($.expr_end_with_leading_whitespace),
    ),

    expr: $ => choice(
      $.literal,
      $.query,
      $.container,
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
