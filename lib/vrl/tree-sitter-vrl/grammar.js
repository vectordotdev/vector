module.exports = grammar({
  name: 'vrl',


  rules: {
      program: $ => $.exprs,

      exprs: $ => seq(
        $.expr,
        repeat(
          seq($._expr_end, $.expr)
        )
      ),

      expr: $ => choice(
        $.literal,
        $.query
      ),

      _expr_end: $ => choice (
        ";",
        "\n"
      ),

      literal: $ => choice(
        $.integer_literal,
        $.float_literal,
      ),

      _integer: $ => seq(
         optional("-"),
         /[0-9_]+/
       ),

      integer_literal: $ => $._integer,

      float_literal: $ => seq(
        $._integer,
        token.immediate("."),
         token.immediate(field("fraction", /[0-9_]+/))
      ),

      query: $ => choice(
        $.local_query
      ),

      local_query: $ => seq(
        $.local_variable,
        optional($.query_segments)
      ),

      local_variable: $ => /[0-9]*[a-zA-Z_][0-9a-zA-Z_]*/,

      query_segments: $ => repeat1(
        seq(
          ".",
          $.query_segment
        )
      ),

      query_segment: $ => /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/
  }

});
