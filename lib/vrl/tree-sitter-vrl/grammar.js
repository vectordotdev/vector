let ws = /\s+/;
let opt_ws = optional(/\s+/);
let ws_horizontal = /\r\n\t\f/;
let opt_ws_horizontal = optional(ws_horizontal);

let whole_number = /[0-9_]+/;

let integer = choice(
  whole_number,
  seq("-", token.immediate(whole_number))
);

let expr_end = choice(
  seq(opt_ws, ";"),
  seq(opt_ws_horizontal, "\n"),
);

let local_name = /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/;

let query_field = choice(
  /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/,
  seq(
    "\"",
    /(\\"|[^"])+/,
    "\"",
  )
);

let query_coalesce = seq(
  "(",
  query_field,
  repeat1(seq(
    "|",
    query_field,
  )),
  ")"
);


seq(
  optional("-"),
  token.immediate(/[0-9_]+/)
);

module.exports = grammar({
  name: 'vrl',

  extras: $ => [],
  conflicts: $ => [[$.dx, $.dxx]],

  rules: {

     program: $ => repeat(choice($.dx, $.dxy, $.dxx)),

    dx: $ => $.x,
    dxy: $ => seq($.x, $.y),
    dxx: $ => seq($.x, $.x),

     x: $ => "x",
     y: $ => "y",
//     xx: $ => "xx",
//     xy: $ => "xy",

//    program: $ => seq(opt_ws, $.exprs),

//    exprs: $ => seq(
//      $.expr,
//      repeat(seq(";", opt_ws, $.expr)),
//      optional(/\s+/)
//      optional(repeat1(
//        seq(expr_end, opt_ws, $.expr)
//      )),
//      optional(expr_end)
//    ),

//    expr: $ => "x",

//    expr: $ => choice(
//      $.literal,
//      $.query
//    ),

//    literal: $ => choice(
//      $.integer_literal,
//      $.float_literal,
//    ),
//
//    integer_literal: $ => integer,
//
//    float_literal: $ => seq(
//      integer,
//      token.immediate("."),
//      token.immediate(whole_number)
//    ),
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
