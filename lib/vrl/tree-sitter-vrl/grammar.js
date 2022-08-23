
let whole_number = /[0-9_]+/;
let local_identifier = /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/;
let query_field_identifier = /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/;
let quoted_field_identifier = /(\\"|[^"])+/;

module.exports = grammar({
  name: 'vrl',

  extras: $ => [],
  inline: $ => [
    $.newline,
    $.expr_end,
    $.ws,
  ],

  conflicts: $ => [[$.ws, $.exprs], [$.newline], [$.ws], [$.exprs], [$.expr_end], [], [$.program]],

  rules: {

    program: $ => seq(optional($.ws), $.exprs, optional($.ws)),

    // horizontal whitespace
    _horizontal_ws: $ => /[ \r\t]+/,

    newline: $ => repeat1(/[\n]/),

    ws: $ => repeat1(prec.left(0, choice($._horizontal_ws, $.newline))),

    expr_end: $ => choice(";", $.newline),

    exprs: $ => seq(
      $.expr,
      repeat(seq(
        optional($.ws),
        $.expr_end,
        optional($.ws),
        $.expr,
      )),
      optional(seq(optional($.ws), $.expr_end)),
    ),

    expr: $ => choice(
      $.literal,
      $.query
    ),

    literal: $ => choice(
      $.float,
      $.integer,
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
      $.local_query
    ),

    local_query: $ => seq(
      $.local_variable,
      optional($.query_segments)
    ),

    local_variable: $ => local_identifier,

    query_segments: $ => repeat1(
      seq(
        ".",
        $._query_segment
      )
    ),

    _query_segment: $ => choice(
      $.query_field,
      $.query_coalesce,
    ),

    _single_quote: $ => "\"",

    quoted_field: $ => quoted_field_identifier,

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


  }
});
