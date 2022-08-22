module.exports = grammar({
  name: 'vrl',


  rules: {
      program: $ => $.exprs,

//      inline: $ => [$.query_field],

      exprs: $ => seq(
        $.expr,
        optional(repeat1(
          seq($._expr_end, $.expr)
        ))
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
         "1"
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
          $._query_segment
        )
      ),

      _query_segment: $ => choice(
        alias($.query_field_immediate, $.query_field),
        $.query_coalesce,
      ),

      query_field: $ => choice(
        /[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/,
        seq(
          "\"",
          /(\\"|[^"])+/,
          "\"",
        )
      ),
      query_field_immediate: $ => choice(
        token.immediate(/[0-9]*[a-zA-Z_@][0-9a-zA-Z_@]*/),
        seq(
          token.immediate("\""),
          /(\\"|[^"])+/,
          "\"",
        )
      ),

      query_coalesce: $ => seq(
        token.immediate("("),
        $.query_field,
        repeat1(seq(
          "|",
          $.query_field,
        )),
        ")"
      ),
  }

});
