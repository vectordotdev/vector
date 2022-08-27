
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
//    [$.exprs],
//    [$.if_statement],
//    [$.predicate_exprs],
//    [$.assignment_expr],
//    [$.predicate_exprs, $.expr],
//
//    [$.literal, $.object_entry],
//    [$.predicate_exprs, $.group],
//    [$.arithmetic_expr_class, $.assignment_target],
//    [$.arithmetic_expr_class, $.assignment_expr_class],
//    [$.arithmetic_expr_class, $.predicate],
//    [$.arithmetic_expr_class],
//    [$.error_coalesce_expr, $.assignment_expr_class],
//    [$.error_coalesce_expr, $.predicate],
//    [$.error_coalesce_expr]
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



  }
});
