#include <tree_sitter/parser.h>

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 13
#define STATE_COUNT 38
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 35
#define ALIAS_COUNT 0
#define TOKEN_COUNT 17
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 0
#define MAX_ALIAS_SEQUENCE_LENGTH 4
#define PRODUCTION_ID_COUNT 1

enum {
  anon_sym_SEMI = 1,
  anon_sym_LF = 2,
  anon_sym_DASH = 3,
  anon_sym_1 = 4,
  anon_sym_DOT = 5,
  aux_sym_float_literal_token1 = 6,
  sym_local_variable = 7,
  anon_sym_DOT2 = 8,
  aux_sym_query_field_token1 = 9,
  anon_sym_DQUOTE = 10,
  aux_sym_query_field_token2 = 11,
  aux_sym_query_field_immediate_token1 = 12,
  anon_sym_DQUOTE2 = 13,
  anon_sym_LPAREN = 14,
  anon_sym_PIPE = 15,
  anon_sym_RPAREN = 16,
  sym_program = 17,
  sym_exprs = 18,
  sym_expr = 19,
  sym__expr_end = 20,
  sym_literal = 21,
  sym__integer = 22,
  sym_integer_literal = 23,
  sym_float_literal = 24,
  sym_query = 25,
  sym_local_query = 26,
  sym_query_segments = 27,
  sym__query_segment = 28,
  sym_query_field = 29,
  sym_query_field_immediate = 30,
  sym_query_coalesce = 31,
  aux_sym_exprs_repeat1 = 32,
  aux_sym_query_segments_repeat1 = 33,
  aux_sym_query_coalesce_repeat1 = 34,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_SEMI] = ";",
  [anon_sym_LF] = "\n",
  [anon_sym_DASH] = "-",
  [anon_sym_1] = "1",
  [anon_sym_DOT] = ".",
  [aux_sym_float_literal_token1] = "float_literal_token1",
  [sym_local_variable] = "local_variable",
  [anon_sym_DOT2] = ".",
  [aux_sym_query_field_token1] = "query_field_token1",
  [anon_sym_DQUOTE] = "\"",
  [aux_sym_query_field_token2] = "query_field_token2",
  [aux_sym_query_field_immediate_token1] = "query_field_immediate_token1",
  [anon_sym_DQUOTE2] = "\"",
  [anon_sym_LPAREN] = "(",
  [anon_sym_PIPE] = "|",
  [anon_sym_RPAREN] = ")",
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
  [sym__query_segment] = "_query_segment",
  [sym_query_field] = "query_field",
  [sym_query_field_immediate] = "query_field",
  [sym_query_coalesce] = "query_coalesce",
  [aux_sym_exprs_repeat1] = "exprs_repeat1",
  [aux_sym_query_segments_repeat1] = "query_segments_repeat1",
  [aux_sym_query_coalesce_repeat1] = "query_coalesce_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_SEMI] = anon_sym_SEMI,
  [anon_sym_LF] = anon_sym_LF,
  [anon_sym_DASH] = anon_sym_DASH,
  [anon_sym_1] = anon_sym_1,
  [anon_sym_DOT] = anon_sym_DOT,
  [aux_sym_float_literal_token1] = aux_sym_float_literal_token1,
  [sym_local_variable] = sym_local_variable,
  [anon_sym_DOT2] = anon_sym_DOT,
  [aux_sym_query_field_token1] = aux_sym_query_field_token1,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [aux_sym_query_field_token2] = aux_sym_query_field_token2,
  [aux_sym_query_field_immediate_token1] = aux_sym_query_field_immediate_token1,
  [anon_sym_DQUOTE2] = anon_sym_DQUOTE,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_PIPE] = anon_sym_PIPE,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
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
  [sym__query_segment] = sym__query_segment,
  [sym_query_field] = sym_query_field,
  [sym_query_field_immediate] = sym_query_field,
  [sym_query_coalesce] = sym_query_coalesce,
  [aux_sym_exprs_repeat1] = aux_sym_exprs_repeat1,
  [aux_sym_query_segments_repeat1] = aux_sym_query_segments_repeat1,
  [aux_sym_query_coalesce_repeat1] = aux_sym_query_coalesce_repeat1,
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
  [anon_sym_1] = {
    .visible = true,
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
  [aux_sym_query_field_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_query_field_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_query_field_immediate_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_DQUOTE2] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PIPE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
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
  [sym__query_segment] = {
    .visible = false,
    .named = true,
  },
  [sym_query_field] = {
    .visible = true,
    .named = true,
  },
  [sym_query_field_immediate] = {
    .visible = true,
    .named = true,
  },
  [sym_query_coalesce] = {
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
  [aux_sym_query_coalesce_repeat1] = {
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
      if (eof) ADVANCE(13);
      if (lookahead == '"') ADVANCE(34);
      if (lookahead == '(') ADVANCE(35);
      if (lookahead == ')') ADVANCE(37);
      if (lookahead == '-') ADVANCE(16);
      if (lookahead == '.') ADVANCE(20);
      if (lookahead == '1') ADVANCE(17);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '@') ADVANCE(33);
      if (lookahead == '_') ADVANCE(22);
      if (lookahead == '|') ADVANCE(36);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(12)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(21);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(32);
      END_STATE();
    case 1:
      if (lookahead == '"') ADVANCE(28);
      if (lookahead == '-') ADVANCE(16);
      if (lookahead == '1') ADVANCE(19);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(1)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(5);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(25);
      END_STATE();
    case 2:
      if (lookahead == '"') ADVANCE(28);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(2)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(7);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(27);
      END_STATE();
    case 3:
      if (lookahead == '@') ADVANCE(27);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(3);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 4:
      if (lookahead == '\\') ADVANCE(31);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(29);
      if (lookahead != 0 &&
          lookahead != '"') ADVANCE(30);
      END_STATE();
    case 5:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(5);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(25);
      END_STATE();
    case 6:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(6);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 7:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(7);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(27);
      END_STATE();
    case 8:
      if (eof) ADVANCE(13);
      if (lookahead == '\n') ADVANCE(15);
      if (lookahead == '"') ADVANCE(34);
      if (lookahead == '(') ADVANCE(35);
      if (lookahead == '.') ADVANCE(26);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(10)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(6);
      if (('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 9:
      if (eof) ADVANCE(13);
      if (lookahead == '\n') ADVANCE(15);
      if (lookahead == '.') ADVANCE(20);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(11)
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(23);
      END_STATE();
    case 10:
      if (eof) ADVANCE(13);
      if (lookahead == '\n') ADVANCE(15);
      if (lookahead == '.') ADVANCE(26);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(10)
      END_STATE();
    case 11:
      if (eof) ADVANCE(13);
      if (lookahead == '\n') ADVANCE(15);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '\t' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(11)
      END_STATE();
    case 12:
      if (eof) ADVANCE(13);
      if (lookahead == '"') ADVANCE(28);
      if (lookahead == ')') ADVANCE(37);
      if (lookahead == '-') ADVANCE(16);
      if (lookahead == '.') ADVANCE(26);
      if (lookahead == '1') ADVANCE(18);
      if (lookahead == ';') ADVANCE(14);
      if (lookahead == '@') ADVANCE(27);
      if (lookahead == '|') ADVANCE(36);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') SKIP(12)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(3);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 13:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 14:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 15:
      ACCEPT_TOKEN(anon_sym_LF);
      if (lookahead == '\n') ADVANCE(15);
      END_STATE();
    case 16:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 17:
      ACCEPT_TOKEN(anon_sym_1);
      if (lookahead == '@') ADVANCE(33);
      if (lookahead == '_') ADVANCE(22);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(21);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(32);
      END_STATE();
    case 18:
      ACCEPT_TOKEN(anon_sym_1);
      if (lookahead == '@') ADVANCE(27);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(3);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 19:
      ACCEPT_TOKEN(anon_sym_1);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(5);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(25);
      END_STATE();
    case 20:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 21:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (lookahead == '@') ADVANCE(33);
      if (lookahead == '_') ADVANCE(22);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(21);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(32);
      END_STATE();
    case 22:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (lookahead == '@') ADVANCE(33);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(22);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(32);
      END_STATE();
    case 23:
      ACCEPT_TOKEN(aux_sym_float_literal_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          lookahead == '_') ADVANCE(23);
      END_STATE();
    case 24:
      ACCEPT_TOKEN(sym_local_variable);
      if (lookahead == '@') ADVANCE(27);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(24);
      END_STATE();
    case 25:
      ACCEPT_TOKEN(sym_local_variable);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(25);
      END_STATE();
    case 26:
      ACCEPT_TOKEN(anon_sym_DOT2);
      END_STATE();
    case 27:
      ACCEPT_TOKEN(aux_sym_query_field_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(27);
      END_STATE();
    case 28:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 29:
      ACCEPT_TOKEN(aux_sym_query_field_token2);
      if (lookahead == '\\') ADVANCE(31);
      if (lookahead == '\t' ||
          lookahead == '\n' ||
          lookahead == '\r' ||
          lookahead == ' ') ADVANCE(29);
      if (lookahead != 0 &&
          lookahead != '"') ADVANCE(30);
      END_STATE();
    case 30:
      ACCEPT_TOKEN(aux_sym_query_field_token2);
      if (lookahead == '\\') ADVANCE(31);
      if (lookahead != 0 &&
          lookahead != '"') ADVANCE(30);
      END_STATE();
    case 31:
      ACCEPT_TOKEN(aux_sym_query_field_token2);
      if (lookahead != 0 &&
          lookahead != '\\') ADVANCE(30);
      if (lookahead == '\\') ADVANCE(31);
      END_STATE();
    case 32:
      ACCEPT_TOKEN(aux_sym_query_field_immediate_token1);
      if (lookahead == '@') ADVANCE(33);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(32);
      END_STATE();
    case 33:
      ACCEPT_TOKEN(aux_sym_query_field_immediate_token1);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('@' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(33);
      END_STATE();
    case 34:
      ACCEPT_TOKEN(anon_sym_DQUOTE2);
      END_STATE();
    case 35:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 36:
      ACCEPT_TOKEN(anon_sym_PIPE);
      END_STATE();
    case 37:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 1},
  [2] = {.lex_state = 1},
  [3] = {.lex_state = 8},
  [4] = {.lex_state = 8},
  [5] = {.lex_state = 8},
  [6] = {.lex_state = 8},
  [7] = {.lex_state = 8},
  [8] = {.lex_state = 8},
  [9] = {.lex_state = 8},
  [10] = {.lex_state = 8},
  [11] = {.lex_state = 8},
  [12] = {.lex_state = 9},
  [13] = {.lex_state = 8},
  [14] = {.lex_state = 9},
  [15] = {.lex_state = 8},
  [16] = {.lex_state = 8},
  [17] = {.lex_state = 2},
  [18] = {.lex_state = 0},
  [19] = {.lex_state = 0},
  [20] = {.lex_state = 2},
  [21] = {.lex_state = 8},
  [22] = {.lex_state = 8},
  [23] = {.lex_state = 8},
  [24] = {.lex_state = 8},
  [25] = {.lex_state = 8},
  [26] = {.lex_state = 0},
  [27] = {.lex_state = 0},
  [28] = {.lex_state = 0},
  [29] = {.lex_state = 0},
  [30] = {.lex_state = 1},
  [31] = {.lex_state = 4},
  [32] = {.lex_state = 0},
  [33] = {.lex_state = 1},
  [34] = {.lex_state = 1},
  [35] = {.lex_state = 4},
  [36] = {.lex_state = 0},
  [37] = {.lex_state = 9},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_SEMI] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [anon_sym_1] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [aux_sym_float_literal_token1] = ACTIONS(1),
    [sym_local_variable] = ACTIONS(1),
    [anon_sym_DOT2] = ACTIONS(1),
    [aux_sym_query_field_token1] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [aux_sym_query_field_immediate_token1] = ACTIONS(1),
    [anon_sym_DQUOTE2] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_PIPE] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
  },
  [1] = {
    [sym_program] = STATE(36),
    [sym_exprs] = STATE(32),
    [sym_expr] = STATE(7),
    [sym_literal] = STATE(24),
    [sym__integer] = STATE(12),
    [sym_integer_literal] = STATE(16),
    [sym_float_literal] = STATE(16),
    [sym_query] = STATE(24),
    [sym_local_query] = STATE(22),
    [anon_sym_DASH] = ACTIONS(3),
    [anon_sym_1] = ACTIONS(5),
    [sym_local_variable] = ACTIONS(7),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 8,
    ACTIONS(3), 1,
      anon_sym_DASH,
    ACTIONS(5), 1,
      anon_sym_1,
    ACTIONS(7), 1,
      sym_local_variable,
    STATE(12), 1,
      sym__integer,
    STATE(22), 1,
      sym_local_query,
    STATE(23), 1,
      sym_expr,
    STATE(16), 2,
      sym_integer_literal,
      sym_float_literal,
    STATE(24), 2,
      sym_literal,
      sym_query,
  [27] = 5,
    ACTIONS(11), 1,
      anon_sym_SEMI,
    ACTIONS(13), 1,
      anon_sym_DOT2,
    STATE(9), 1,
      aux_sym_query_segments_repeat1,
    STATE(21), 1,
      sym_query_segments,
    ACTIONS(9), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [44] = 4,
    ACTIONS(15), 1,
      aux_sym_query_field_immediate_token1,
    ACTIONS(17), 1,
      anon_sym_DQUOTE2,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    STATE(13), 3,
      sym__query_segment,
      sym_query_field_immediate,
      sym_query_coalesce,
  [59] = 5,
    ACTIONS(21), 1,
      ts_builtin_sym_end,
    ACTIONS(23), 1,
      anon_sym_SEMI,
    ACTIONS(25), 1,
      anon_sym_LF,
    STATE(2), 1,
      sym__expr_end,
    STATE(6), 1,
      aux_sym_exprs_repeat1,
  [75] = 5,
    ACTIONS(27), 1,
      ts_builtin_sym_end,
    ACTIONS(29), 1,
      anon_sym_SEMI,
    ACTIONS(32), 1,
      anon_sym_LF,
    STATE(2), 1,
      sym__expr_end,
    STATE(6), 1,
      aux_sym_exprs_repeat1,
  [91] = 5,
    ACTIONS(23), 1,
      anon_sym_SEMI,
    ACTIONS(25), 1,
      anon_sym_LF,
    ACTIONS(35), 1,
      ts_builtin_sym_end,
    STATE(2), 1,
      sym__expr_end,
    STATE(5), 1,
      aux_sym_exprs_repeat1,
  [107] = 4,
    ACTIONS(39), 1,
      anon_sym_SEMI,
    ACTIONS(41), 1,
      anon_sym_DOT2,
    STATE(8), 1,
      aux_sym_query_segments_repeat1,
    ACTIONS(37), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [121] = 4,
    ACTIONS(13), 1,
      anon_sym_DOT2,
    ACTIONS(46), 1,
      anon_sym_SEMI,
    STATE(8), 1,
      aux_sym_query_segments_repeat1,
    ACTIONS(44), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [135] = 2,
    ACTIONS(48), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
    ACTIONS(50), 2,
      anon_sym_SEMI,
      anon_sym_DOT2,
  [144] = 2,
    ACTIONS(52), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
    ACTIONS(54), 2,
      anon_sym_SEMI,
      anon_sym_DOT2,
  [153] = 3,
    ACTIONS(58), 1,
      anon_sym_SEMI,
    ACTIONS(60), 1,
      anon_sym_DOT,
    ACTIONS(56), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [164] = 2,
    ACTIONS(37), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
    ACTIONS(39), 2,
      anon_sym_SEMI,
      anon_sym_DOT2,
  [173] = 2,
    ACTIONS(64), 1,
      anon_sym_SEMI,
    ACTIONS(62), 3,
      ts_builtin_sym_end,
      anon_sym_LF,
      anon_sym_DOT,
  [182] = 2,
    ACTIONS(66), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
    ACTIONS(68), 2,
      anon_sym_SEMI,
      anon_sym_DOT2,
  [191] = 2,
    ACTIONS(72), 1,
      anon_sym_SEMI,
    ACTIONS(70), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [199] = 3,
    ACTIONS(74), 1,
      aux_sym_query_field_token1,
    ACTIONS(76), 1,
      anon_sym_DQUOTE,
    STATE(29), 1,
      sym_query_field,
  [209] = 3,
    ACTIONS(78), 1,
      anon_sym_PIPE,
    ACTIONS(81), 1,
      anon_sym_RPAREN,
    STATE(18), 1,
      aux_sym_query_coalesce_repeat1,
  [219] = 3,
    ACTIONS(83), 1,
      anon_sym_PIPE,
    ACTIONS(85), 1,
      anon_sym_RPAREN,
    STATE(18), 1,
      aux_sym_query_coalesce_repeat1,
  [229] = 3,
    ACTIONS(74), 1,
      aux_sym_query_field_token1,
    ACTIONS(76), 1,
      anon_sym_DQUOTE,
    STATE(27), 1,
      sym_query_field,
  [239] = 2,
    ACTIONS(89), 1,
      anon_sym_SEMI,
    ACTIONS(87), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [247] = 2,
    ACTIONS(93), 1,
      anon_sym_SEMI,
    ACTIONS(91), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [255] = 2,
    ACTIONS(95), 1,
      anon_sym_SEMI,
    ACTIONS(27), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [263] = 2,
    ACTIONS(99), 1,
      anon_sym_SEMI,
    ACTIONS(97), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [271] = 2,
    ACTIONS(103), 1,
      anon_sym_SEMI,
    ACTIONS(101), 2,
      ts_builtin_sym_end,
      anon_sym_LF,
  [279] = 1,
    ACTIONS(105), 2,
      anon_sym_PIPE,
      anon_sym_RPAREN,
  [284] = 2,
    ACTIONS(83), 1,
      anon_sym_PIPE,
    STATE(19), 1,
      aux_sym_query_coalesce_repeat1,
  [291] = 1,
    ACTIONS(107), 2,
      anon_sym_PIPE,
      anon_sym_RPAREN,
  [296] = 1,
    ACTIONS(81), 2,
      anon_sym_PIPE,
      anon_sym_RPAREN,
  [301] = 1,
    ACTIONS(109), 1,
      anon_sym_DQUOTE,
  [305] = 1,
    ACTIONS(111), 1,
      aux_sym_query_field_token2,
  [309] = 1,
    ACTIONS(113), 1,
      ts_builtin_sym_end,
  [313] = 1,
    ACTIONS(115), 1,
      anon_sym_DQUOTE,
  [317] = 1,
    ACTIONS(117), 1,
      anon_sym_1,
  [321] = 1,
    ACTIONS(119), 1,
      aux_sym_query_field_token2,
  [325] = 1,
    ACTIONS(121), 1,
      ts_builtin_sym_end,
  [329] = 1,
    ACTIONS(123), 1,
      aux_sym_float_literal_token1,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 27,
  [SMALL_STATE(4)] = 44,
  [SMALL_STATE(5)] = 59,
  [SMALL_STATE(6)] = 75,
  [SMALL_STATE(7)] = 91,
  [SMALL_STATE(8)] = 107,
  [SMALL_STATE(9)] = 121,
  [SMALL_STATE(10)] = 135,
  [SMALL_STATE(11)] = 144,
  [SMALL_STATE(12)] = 153,
  [SMALL_STATE(13)] = 164,
  [SMALL_STATE(14)] = 173,
  [SMALL_STATE(15)] = 182,
  [SMALL_STATE(16)] = 191,
  [SMALL_STATE(17)] = 199,
  [SMALL_STATE(18)] = 209,
  [SMALL_STATE(19)] = 219,
  [SMALL_STATE(20)] = 229,
  [SMALL_STATE(21)] = 239,
  [SMALL_STATE(22)] = 247,
  [SMALL_STATE(23)] = 255,
  [SMALL_STATE(24)] = 263,
  [SMALL_STATE(25)] = 271,
  [SMALL_STATE(26)] = 279,
  [SMALL_STATE(27)] = 284,
  [SMALL_STATE(28)] = 291,
  [SMALL_STATE(29)] = 296,
  [SMALL_STATE(30)] = 301,
  [SMALL_STATE(31)] = 305,
  [SMALL_STATE(32)] = 309,
  [SMALL_STATE(33)] = 313,
  [SMALL_STATE(34)] = 317,
  [SMALL_STATE(35)] = 321,
  [SMALL_STATE(36)] = 325,
  [SMALL_STATE(37)] = 329,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [5] = {.entry = {.count = 1, .reusable = false}}, SHIFT(12),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [9] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_local_query, 1),
  [11] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_local_query, 1),
  [13] = {.entry = {.count = 1, .reusable = false}}, SHIFT(4),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [21] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_exprs, 2),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [25] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [27] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_exprs_repeat1, 2),
  [29] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_exprs_repeat1, 2), SHIFT_REPEAT(2),
  [32] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_exprs_repeat1, 2), SHIFT_REPEAT(2),
  [35] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_exprs, 1),
  [37] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_query_segments_repeat1, 2),
  [39] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_query_segments_repeat1, 2),
  [41] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_query_segments_repeat1, 2), SHIFT_REPEAT(4),
  [44] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_segments, 1),
  [46] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query_segments, 1),
  [48] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_coalesce, 4),
  [50] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query_coalesce, 4),
  [52] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_field_immediate, 3),
  [54] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query_field_immediate, 3),
  [56] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_integer_literal, 1),
  [58] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_integer_literal, 1),
  [60] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [62] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__integer, 2),
  [64] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__integer, 2),
  [66] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_field_immediate, 1),
  [68] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query_field_immediate, 1),
  [70] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_literal, 1),
  [72] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_literal, 1),
  [74] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [76] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [78] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_query_coalesce_repeat1, 2), SHIFT_REPEAT(17),
  [81] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_query_coalesce_repeat1, 2),
  [83] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [85] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [87] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_local_query, 2),
  [89] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_local_query, 2),
  [91] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query, 1),
  [93] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_query, 1),
  [95] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_exprs_repeat1, 2),
  [97] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expr, 1),
  [99] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_expr, 1),
  [101] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_float_literal, 3),
  [103] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_float_literal, 3),
  [105] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_field, 1),
  [107] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_query_field, 3),
  [109] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [111] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [113] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_program, 1),
  [115] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [117] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [119] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [121] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [123] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
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
