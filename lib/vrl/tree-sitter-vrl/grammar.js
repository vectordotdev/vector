//let ws = /\s+/;
//let optws = optional(/\s+/);
//let ws_horizontal = /\r\n\t\f/;
//let optws_horizontal = optional(ws_horizontal);
//
//let whole_number = /[0-9_]+/;
//
//let integer = choice(
//  whole_number,
//  seq("-", token.immediate(whole_number))
//);
//
//let expr_end = choice(
//  seq(optws, ";"),
//  seq(optws_horizontal, "\n"),
//);
//
//let local_name = /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/;
//
//let query_field = choice(
//  /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/,
//  seq(
//    "\"",
//    /(\\"|[^"])+/,
//    "\"",
//  )
//);
//
//let query_coalesce = seq(
//  "(",
//  query_field,
//  repeat1(seq(
//    "|",
//    query_field,
//  )),
//  ")"
//);


//seq(
//  optional("-"),
//  token.immediate(/[0-9_]+/)
//);
let whole_number = /[0-9_]+/;

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

    ws: $ => repeat1(choice($._horizontal_ws, $.newline)),

    expr_end: $ => choice(";", $.newline),

    exprs: $ => seq(
      $.expr,
      repeat(seq(
        optional($.ws),
        $.expr_end,
        optional($.ws),
        $.expr,
      )),
      optional($.expr_end),
    ),

    expr: $ => choice(
      $.literal,
//      $.query
    ),

    literal: $ => choice(
      $.float,
      $.integer,
    ),

    _integer: $ => seq(optional("-"), $._whole_number),

    integer: $ => $._integer,

    whole_number: $ => whole_number,
    _whole_number: $ =>$.whole_number,

    float: $ => seq(
      $.integer,
      ".",
      $.whole_number,
    ),
//
//    query: $ => choice(
//      $.local_query
//    ),
//
//    local_query: $ => seq(
//      $.local_variable,
//      optional($.query_segments)
//    ),
//
//    local_variable: $ => local_name,
//
//    query_segments: $ => repeat1(
//      seq(
//        token.immediate("."),
//        $._query_segment
//      )
//    ),
//
//    _query_segment: $ => choice(
//      $.query_field,
//      token.immediate(query_coalesce),
//    ),
//
//    query_field: $ => query_field,
  }
});
