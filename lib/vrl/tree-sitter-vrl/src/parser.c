#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 13
#define STATE_COUNT 23
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 23
#define ALIAS_COUNT 0
#define TOKEN_COUNT 10
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 0
#define MAX_ALIAS_SEQUENCE_LENGTH 3
#define PRODUCTION_ID_COUNT 1

enum {
  anon_sym_SEMI = 1,
  anon_sym_LF = 2,
  anon_sym_DASH = 3,
  aux_sym__integer_token1 = 4,
  anon_sym_DOT = 5,
  aux_sym_float_literal_token1 = 6,
  sym_local_variable = 7,
  anon_sym_DOT2 = 8,
  sym_query_segment = 9,
  sym_program = 10,
  sym_exprs = 11,
  sym_expr = 12,
  sym__expr_end = 13,
  sym_literal = 14,
  sym__integer = 15,
  sym_integer_literal = 16,
  sym_float_literal = 17,
  sym_query = 18,
  sym_local_query = 19,
  sym_query_segments = 20,
  aux_sym_exprs_repeat1 = 21,
  aux_sym_query_segments_repeat1 = 22,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_SEMI] = ";",
  [anon_sym_LF] = "\n",
  [anon_sym_DASH] = "-",
  [aux_sym__integer_token1] = "_integer_token1",
  [anon_sym_DOT] = ".",
  [aux_sym_float_literal_token1] = "float_literal_token1",
  [sym_local_variable] = "local_variable",
  [anon_sym_DOT2] = ".",
  [sym_query_segment] = "query_segment",
  [sym_program] = "program",
  [sym_exprs] = "exprs",
  [sym_expr] = "expr",
  [sym__expr_end] = "_expr_end",
  [sym_literal] = "literal",
  [sym__integer] = "_integer",
  [sym_integer_literal] = "integer_literal",
  [sym_float_literal] = "float_literal",
  [sym_query] = "query",
  [sym_local_query] = "local_query",
  [sym_query_segments] = "query_segments",
  [aux_sym_exprs_repeat1] = "exprs_repeat1",
  [aux_sym_query_segments_repeat1] = "query_segments_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_SEMI] = anon_sym_SEMI,
  [anon_sym_LF] = anon_sym_LF,
  [anon_sym_DASH] = anon_sym_DASH,
  [aux_sym__integer_token1] = aux_sym__integer_token1,
  [anon_sym_DOT] = anon_sym_DOT,
  [aux_sym_float_literal_token1] = aux_sym_float_literal_token1,
  [sym_local_variable] = sym_local_variable,
  [anon_sym_DOT2] = anon_sym_DOT,
  [sym_query_segment] = sym_query_segment,
  [sym_program] = sym_program,
  [sym_exprs] = sym_exprs,
  [sym_expr] = sym_expr,
  [sym__expr_end] = sym__expr_end,
  [sym_literal] = sym_literal,
  [sym__integer] = sym__integer,
  [sym_integer_literal] = sym_integer_literal,
  [sym_float_literal] = sym_float_literal,
  [sym_query] = sym_query,
  [sym_local_query] = sym_local_query,
  [sym_query_segments] = sym_query_segments,
  [aux_sym_exprs_repeat1] = aux_sym_exprs_repeat1,
  [aux_sym_query_segments_repeat1] = aux_sym_query_segments_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_SEMI] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LF] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [aux_sym__integer_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_float_literal_token1] = {
    .visible = false,
    .named = false,
  },
  [sym_local_variable] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DOT2] = {
    .visible = true,
    .named = false,
  },
  [sym_query_segment] = {
    .visible = true,
    .named = true,
  },
  [sym_program] = {
    .visible = true,
    .named = true,
  },
  [sym_exprs] = {
    .visible = true,
    .named = true,
  },
  [sym_expr] = {
    .visible = true,
    .named = true,
  },
  [sym__expr_end] = {
    .visible = false,
    .named = true,
  },
  [sym_literal] = {
    .visible = true,
    .named = true,
  },
  [sym__integer] = {
    .visible = false,
    .named = true,
  },
  [sym_integer_literal] = {
    .visible = true,
    .named = true,
  },
  [sym_float_literal] = {
    .visible = true,
    .named = true,
  },
  [sym_query] = {
    .visible = true,
    .named = true,
  },
  [sym_local_query] = {
    .visible = true,
    .named = true,
  },
  [sym_query_segments] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_exprs_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_query_segments_repeat1] = {
    .visible = false,
    .named = false,
  },
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
};

static const uint16_t ts_non_terminal_alias_map[] = {
  0,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(9);
      if (lookahead == '-') ADVANCE(12);
      if (lookahead == '.') ADVANCE(17);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '@') ADVANCE(24);
      if (lookahead == '_') ADVANCE(19);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(8)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(18);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 1:
      if (lookahead == '-') ADVANCE(12);
      if (lookahead == '_') ADVANCE(16);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(1)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(15);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(22);
      END_STATE();
    case 2:
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(2)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(3);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 3:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(3);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 4:
      if (eof) ADVANCE(9);
      if (lookahead == '\n') ADVANCE(11);
      if (lookahead == '.') ADVANCE(17);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(7)
      END_STATE();
    case 5:
      if (eof) ADVANCE(9);
      if (lookahead == '\n') ADVANCE(11);
      if (lookahead == '.') ADVANCE(23);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(5)
      END_STATE();
    case 6:
      if (eof) ADVANCE(9);
      if (lookahead == '\n') ADVANCE(11);
      if (lookahead == '.') ADVANCE(23);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(5)
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(20);
      END_STATE();
    case 7:
      if (eof) ADVANCE(9);
      if (lookahead == '\n') ADVANCE(11);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(7)
      END_STATE();
    case 8:
      if (eof) ADVANCE(9);
      if (lookahead == '-') ADVANCE(12);
      if (lookahead == '.') ADVANCE(23);
      if (lookahead == ';') ADVANCE(10);
      if (lookahead == '@') ADVANCE(24);
      if (lookahead == '_') ADVANCE(14);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(8)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(13);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 9:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 10:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 11:
      ACCEPT_TOKEN(anon_sym_LF);
      if (lookahead == '\n') ADVANCE(11);
      END_STATE();
    case 12:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 13:
      ACCEPT_TOKEN(aux_sym__integer_token1);
      if (lookahead == '@') ADVANCE(24);
      if (lookahead == '_') ADVANCE(14);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(13);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 14:
      ACCEPT_TOKEN(aux_sym__integer_token1);
      if (lookahead == '@') ADVANCE(24);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(14);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 15:
      ACCEPT_TOKEN(aux_sym__integer_token1);
      if (lookahead == '_') ADVANCE(16);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(15);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(22);
      END_STATE();
    case 16:
      ACCEPT_TOKEN(aux_sym__integer_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(16);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(22);
      END_STATE();
    case 17:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 18:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (lookahead == '@') ADVANCE(24);
      if (lookahead == '_') ADVANCE(19);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(18);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (lookahead == '@') ADVANCE(24);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(19);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 20:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(20);
      END_STATE();
    case 21:
      ACCEPT_TOKEN(sym_local_variable);
      if (lookahead == '@') ADVANCE(24);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(21);
      END_STATE();
    case 22:
      ACCEPT_TOKEN(sym_local_variable);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(22);
      END_STATE();
    case 23:
      ACCEPT_TOKEN(anon_sym_DOT2);
      END_STATE();
    case 24:
      ACCEPT_TOKEN(sym_query_segment);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 1},
  [2] = {.lex_state = 1},
  [3] = {.lex_state = 6},
  [4] = {.lex_state = 6},
  [5] = {.lex_state = 6},
  [6] = {.lex_state = 6},
  [7] = {.lex_state = 6},
  [8] = {.lex_state = 6},
  [9] = {.lex_state = 6},
  [10] = {.lex_state = 4},
  [11] = {.lex_state = 4},
  [12] = {.lex_state = 6},
  [13] = {.lex_state = 6},
  [14] = {.lex_state = 6},
  [15] = {.lex_state = 6},
  [16] = {.lex_state = 6},
  [17] = {.lex_state = 6},
  [18] = {.lex_state = 2},
  [19] = {.lex_state = 1},
  [20] = {.lex_state = 6},
  [21] = {.lex_state = 0},
  [22] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_SEMI] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [aux_sym__integer_token1] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [aux_sym_float_literal_token1] = ACTIONS(1),
    [sym_local_variable] = ACTIONS(1),
    [anon_sym_DOT2] = ACTIONS(1),
    [sym_query_segment] = ACTIONS(1),
  },
  [1] = {
    [sym_program] = STATE(22),
    [sym_exprs] = STATE(21),
    [sym_expr] = STATE(7),
    [sym_literal] = STATE(15),
    [sym__integer] = STATE(10),
    [sym_integer_literal] = STATE(14),
    [sym_float_literal] = STATE(14),
    [sym_query] = STATE(15),
    [sym_local_query] = STATE(12),
    [anon_sym_DASH] = ACTIONS(3),
    [aux_sym__integer_token1] = ACTIONS(5),
    [sym_local_variable] = ACTIONS(7),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 8,
    ACTIONS(3), 1,
      anon_sym_DASH,
    ACTIONS(5), 1,
      aux_sym__integer_token1,
    ACTIONS(7), 1,
      sym_local_variable,
    STATE(10), 1,
      sym__integer,
    STATE(12), 1,
      sym_local_query,
    STATE(16), 1,
      sym_expr,
    STATE(14), 2,
      sym_integer_literal,
      sym_float_literal,
    STATE(15), 2,
      sym_literal,
      sym_query,
  [27] = 5,
    ACTIONS(11), 1,
      anon_sym_SEMI,
    ACTIONS(13), 1,
      anon_sym_DOT2,
    STATE(4), 1,
      aux_sym_query_segments_repeat1,
    STATE(13), 1,
      sym_query_segments,
    ACTIONS(9), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [44] = 4,
    ACTIONS(13), 1,
      anon_sym_DOT2,
    ACTIONS(17), 1,
      anon_sym_SEMI,
    STATE(6), 1,
      aux_sym_query_segments_repeat1,
    ACTIONS(15), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [58] = 5,
    ACTIONS(19), 1,
      ts_builtin_sym_end,
    ACTIONS(21), 1,
      anon_sym_SEMI,
    ACTIONS(24), 1,
      anon_sym_LF,
    STATE(2), 1,
      sym__expr_end,
    STATE(5), 1,
      aux_sym_exprs_repeat1,
  [74] = 4,
    ACTIONS(29), 1,
      anon_sym_SEMI,
    ACTIONS(31), 1,
      anon_sym_DOT2,
    STATE(6), 1,
      aux_sym_query_segments_repeat1,
    ACTIONS(27), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [88] = 5,
    ACTIONS(34), 1,
      ts_builtin_sym_end,
    ACTIONS(36), 1,
      anon_sym_SEMI,
    ACTIONS(38), 1,
      anon_sym_LF,
    STATE(2), 1,
      sym__expr_end,
    STATE(8), 1,
      aux_sym_exprs_repeat1,
  [104] = 5,
    ACTIONS(36), 1,
      anon_sym_SEMI,
    ACTIONS(38), 1,
      anon_sym_LF,
    ACTIONS(40), 1,
      ts_builtin_sym_end,
    STATE(2), 1,
      sym__expr_end,
    STATE(5), 1,
      aux_sym_exprs_repeat1,
  [120] = 2,
    ACTIONS(27), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
    ACTIONS(29), 2,
      anon_sym_SEMI,
      anon_sym_DOT2,
  [129] = 3,
    ACTIONS(44), 1,
      anon_sym_SEMI,
    ACTIONS(46), 1,
      anon_sym_DOT,
    ACTIONS(42), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [140] = 2,
    ACTIONS(50), 1,
      anon_sym_SEMI,
    ACTIONS(48), 3,
      ts_builtin_sym_end,
      anon_sym_LF,
      anon_sym_DOT,
  [149] = 2,
    ACTIONS(54), 1,
      anon_sym_SEMI,
    ACTIONS(52), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [157] = 2,
    ACTIONS(58), 1,
      anon_sym_SEMI,
    ACTIONS(56), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [165] = 2,
    ACTIONS(62), 1,
      anon_sym_SEMI,
    ACTIONS(60), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [173] = 2,
    ACTIONS(66), 1,
      anon_sym_SEMI,
    ACTIONS(64), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [181] = 2,
    ACTIONS(68), 1,
      anon_sym_SEMI,
    ACTIONS(19), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [189] = 2,
    ACTIONS(72), 1,
      anon_sym_SEMI,
    ACTIONS(70), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [197] = 1,
    ACTIONS(74), 1,
      sym_query_segment,
  [201] = 1,
    ACTIONS(76), 1,
      aux_sym__integer_token1,
  [205] = 1,
    ACTIONS(78), 1,
      aux_sym_float_literal_token1,
  [209] = 1,
    ACTIONS(80), 1,
      ts_builtin_sym_end,
  [213] = 1,
    ACTIONS(82), 1,
      ts_builtin_sym_end,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 27,
  [SMALL_STATE(4)] = 44,
  [SMALL_STATE(5)] = 58,
  [SMALL_STATE(6)] = 74,
  [SMALL_STATE(7)] = 88,
  [SMALL_STATE(8)] = 104,
  [SMALL_STATE(9)] = 120,
  [SMALL_STATE(10)] = 129,
  [SMALL_STATE(11)] = 140,
  [SMALL_STATE(12)] = 149,
  [SMALL_STATE(13)] = 157,
  [SMALL_STATE(14)] = 165,
  [SMALL_STATE(15)] = 173,
  [SMALL_STATE(16)] = 181,
  [SMALL_STATE(17)] = 189,
  [SMALL_STATE(18)] = 197,
  [SMALL_STATE(19)] = 201,
  [SMALL_STATE(20)] = 205,
  [SMALL_STATE(21)] = 209,
  [SMALL_STATE(22)] = 213,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [5] = {.entry = {.count = 1, .reusable = false}}, SHIFT(10),
  [7] = {.entry = {.count = 1, .reusable = false}}, SHIFT(3),
  [9] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_local_query, 1),
  [11] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_local_query, 1),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(18),
  [15] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_segments, 1),
  [17] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query_segments, 1),
  [19] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_exprs_repeat1, 2),
  [21] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_exprs_repeat1, 2), SHIFT_REPEAT(2),
  [24] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_exprs_repeat1, 2), SHIFT_REPEAT(2),
  [27] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_query_segments_repeat1, 2),
  [29] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_query_segments_repeat1, 2),
  [31] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_query_segments_repeat1, 2), SHIFT_REPEAT(18),
  [34] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_exprs, 1),
  [36] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [38] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [40] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_exprs, 2),
  [42] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_integer_literal, 1),
  [44] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_integer_literal, 1),
  [46] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [48] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__integer, 2),
  [50] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__integer, 2),
  [52] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query, 1),
  [54] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query, 1),
  [56] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_local_query, 2),
  [58] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_local_query, 2),
  [60] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_literal, 1),
  [62] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_literal, 1),
  [64] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expr, 1),
  [66] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_expr, 1),
  [68] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_exprs_repeat1, 2),
  [70] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_float_literal, 3),
  [72] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_float_literal, 3),
  [74] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [76] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [78] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [80] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_program, 1),
  [82] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_vrl(void) {
  static const TSLanguage language = {
    .version = LANGUAGE_VERSION,
    .symbol_count = SYMBOL_COUNT,
    .alias_count = ALIAS_COUNT,
    .token_count = TOKEN_COUNT,
    .external_token_count = EXTERNAL_TOKEN_COUNT,
    .state_count = STATE_COUNT,
    .large_state_count = LARGE_STATE_COUNT,
    .production_id_count = PRODUCTION_ID_COUNT,
    .field_count = FIELD_COUNT,
    .max_alias_sequence_length = MAX_ALIAS_SEQUENCE_LENGTH,
    .parse_table = &ts_parse_table[0][0],
    .small_parse_table = ts_small_parse_table,
    .small_parse_table_map = ts_small_parse_table_map,
    .parse_actions = ts_parse_actions,
    .symbol_names = ts_symbol_names,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
