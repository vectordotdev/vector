// VRL Language Definition
export let vrlLanguageDefinition = {
    defaultToken: "invalid",
    ignoreCase: true,
    tokenPostfix: ".vrl",

    brackets: [
        { open: "{", close: "}", token: "delimiter.curly" },
        { open: "[", close: "]", token: "delimiter.square" },
        { open: "(", close: ")", token: "delimiter.parenthesis" },
    ],

    regEx: /\/(?!\/\/)(?:[^\/\\]|\\.)*\/[igm]*/,

    keywords: [
        "abort",
        "as",
        "break",
        "continue",
        "else",
        "false",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "null",
        "return",
        "self",
        "std",
        "then",
        "this",
        "true",
        "type",
        "until",
        "use",
        "while",
    ],

    // we include these common regular expressions
    symbols: /[=><!~?&%|+\-*\/\^\.,\:]+/,
    escapes: /\\(?:[abfnrtv\\"'$]|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

    // The main tokenizer for our languages
    tokenizer: {
        root: [
            // function invokes fallible
            [
                /([a-zA-Z_!]+)(\!)(\()/,
                {
                    cases: {
                        $3: ['keyword', 'keyword', '']
                    }
                    //   log: 'in function invoke::found::\n\n$0'
                },
            ],
            // function invokes
            [
                /([a-zA-Z_!]+)(\()/,
                {
                    cases: {
                        $2: ['keyword', '']
                    }
                    //   log: 'in function invoke::found::\n\n$0'
                },
            ],

            // rstrings
            [/r'[^']+'/, { token: "regexp" /*log: 'root_r_string::\n\n$0'*/ }],

            // timestamps
            [/t'[^']+'/, { token: "regexp" /*log: 'function_arg_r_string::\n\n$0'*/ }],

            // field access, eg: .foo
            [/(\.[^\ \=]+)([\ |\=])/, {
                cases: {
                    $2: ["entity", ""]
                }
            }],

            // sstrings
            [/s'[^']+'/, { token: "string" /*log: 'root_s_string::\n\n$0'*/ }],

            // identifiers and keywords
            [/\@[a-zA-Z_]\w*/, "variable.predefined"],

            [
                /[a-zA-Z_]\w*/,
                {
                    cases: {
                    this: "variable.predefined",
                    "@keywords": { token: "keyword.$0" },

                    "@default": "",
                    },
                },
            ],

            // whitespace
            [/[ \t\r\n]+/, ""],

            // Comments
            [/###/, "comment", "@comment"],
            [/#.*$/, "comment"],

            // Function invokes

            // regular expressions
            ["///", { token: "regexp", next: "@hereregexp" }],

            [/^(\s*)(@regEx)/, ["", "regexp"]],
            [/(\()(\s*)(@regEx)/, ["@brackets", "", "regexp"]],
            [/(\,)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\=)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\:)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\[)(\s*)(@regEx)/, ["@brackets", "", "regexp"]],
            [/(\!)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\&)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\|)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\?)(\s*)(@regEx)/, ["delimiter", "", "regexp"]],
            [/(\{)(\s*)(@regEx)/, ["@brackets", "", "regexp"]],
            [/(\;)(\s*)(@regEx)/, ["", "", "regexp"]],

            // delimiters
            [
                /}/,
                {
                    cases: {
                    "$S2==interpolatedstring": {
                        token: "string",
                        next: "@pop",
                    },
                    "@default": "@brackets",
                    },
                },
            ],
            [/[{}()\[\]]/, "@brackets"],
            [/@symbols/, "delimiter"],

            // numbers
            [/\d+[eE]([\-+]?\d+)?/, "number.float"],
            [/\d+\.\d+([eE][\-+]?\d+)?/, "number.float"],
            [/0[xX][0-9a-fA-F]+/, "number.hex"],
            [/0[0-7]+(?!\d)/, "number.octal"],
            [/\d+/, "number"],

            // delimiter: after number because of .\d floats
            [/[,.]/, "delimiter"],

            // strings:
            [/"""/, "string", '@herestring."""'],
            [/'''/, "string", "@herestring.'''"],
            [
                /"/,
                {
                    cases: {
                    "@eos": "string",
                    "@default": { token: "string", next: '@string."' },
                    },
                },
            ],
            [
                /'/,
                {
                    cases: {
                    "@eos": "string",
                    "@default": { token: "string", next: "@string.'" },
                    },
                },
            ],
        ],

        string: [
            [/[^"'\#\\]+/, "string"],
            [/@escapes/, "string.escape"],
            [/\./, "string.escape.invalid"],
            [/\./, "string.escape.invalid"],

            [
                /#{/,
                {
                    cases: {
                    '$S2=="': {
                        token: "string",
                        next: "root.interpolatedstring",
                    },
                    "@default": "string",
                    },
                },
            ],

            [
                /["']/,
                {
                    cases: {
                    "$#==$S2": { token: "string", next: "@pop" },
                    "@default": "string",
                    },
                },
            ],
            [/#/, "string"],
        ],

        herestring: [
            [
                /("""|''')/,
                {
                    cases: {
                    "$1==$S2": { token: "string", next: "@pop" },
                    "@default": "string",
                    },
                },
            ],
            [/[^#\\'"]+/, "string"],
            [/['"]+/, "string"],
            [/@escapes/, "string.escape"],
            [/\./, "string.escape.invalid"],

            [/#{/, { token: "string.quote", next: "root.interpolatedstring" }],
            [/#/, "string"],
        ],

        comment: [
            [/[^#]+/, "comment"],
            [/###/, "comment", "@pop"],
            [/#/, "comment"],
        ],

        hereregexp: [
            [/[^\\\/#]+/, "regexp"],
            [/\\./, "regexp"],
            [/#.*$/, "comment"],
            ["///[igm]*", { token: "regexp", next: "@pop" }],
            [/\//, "regexp"],
        ],

        function_arg: [
            [
                /([a-zA-Z_!]+)\(/,
                { cases: { $S1: { token: "keyword", next: "@push", log: "function_arg_nested::\n\n$0" } } },
            ],
            // we copy the string highlighters from root and apply them to function arguments as well
            [/"""/, "string", '@herestring."""'],
            [/'''/, "string", "@herestring.'''"],
            [
                /"/,
                {
                    cases: {
                    "@eos": "string",
                    "@default": { token: "string", next: '@string."' },
                    },
                },
            ],
            [
                /'/,
                {
                    cases: {
                    "@eos": "string",
                    "@default": { token: "string", next: "@string.'" },
                    },
                },
            ],
            // TODO: field access inside function parameters sometimes fails, eg: in a
            // newline func(.foo)
            // [/(\.)/, { token: "entity" }],
            // r strings
            [/r'[^']+'/, { token: "regexp" /*log: 'function_arg_r_string::\n\n$0'*/ }],
            [/t'[^']+'/, { token: "regexp" /*log: 'function_arg_r_string::\n\n$0'*/ }],
            // s strings
            [/s'[^']+'/, { token: "string" /*log: 'root_s_string::\n\n$0'*/ }],
            [/\)/, { token: "", /*log: 'end_off_function_arg::\n\n$0',*/ next: "@pop" }],
            [
            /[\.\[\]\,\\\"\%\{\}\$\:\^\w]+/,
            { token: "" /*log: 'function_arg_anything_else::\n\n$0'*/ },
            ],
        ],
    },
};

export let vrlThemeDefinition = {
    "base": "vs",
    "inherit": true,
    "rules": [
      {
        "background": "ffffff",
        "token": ""
      },
      {
        "foreground": "d73a49",
        "token": "vrl-function-invokes"
      },
      {
        "foreground": "d73a49",
        "token": "vrl-function-invokes-fallible"
      },
      {
        "foreground": "6a737d",
        "token": "comment"
      },
      {
        "foreground": "6a737d",
        "token": "punctuation.definition.comment"
      },
      {
        "foreground": "6a737d",
        "token": "string.comment"
      },
      {
        "foreground": "005cc5",
        "token": "constant"
      },
      {
        "foreground": "005cc5",
        "token": "entity.name.constant"
      },
      {
        "foreground": "005cc5",
        "token": "variable.other.constant"
      },
      {
        "foreground": "005cc5",
        "token": "variable.language"
      },
      {
        "foreground": "6f42c1",
        "token": "entity"
      },
      {
        "foreground": "6f42c1",
        "token": "entity.name"
      },
      {
        "foreground": "24292e",
        "token": "variable.parameter.function"
      },
      {
        "foreground": "22863a",
        "token": "entity.name.tag"
      },
      {
        "foreground": "d73a49",
        "token": "keyword"
      },
      {
        "foreground": "d73a49",
        "token": "storage"
      },
      {
        "foreground": "d73a49",
        "token": "storage.type"
      },
      {
        "foreground": "24292e",
        "token": "storage.modifier.package"
      },
      {
        "foreground": "24292e",
        "token": "storage.modifier.import"
      },
      {
        "foreground": "24292e",
        "token": "storage.type.java"
      },
      {
        "foreground": "032f62",
        "token": "string"
      },
      {
        "foreground": "032f62",
        "token": "punctuation.definition.string"
      },
      {
        "foreground": "032f62",
        "token": "string punctuation.section.embedded source"
      },
      {
        "foreground": "005cc5",
        "token": "support"
      },
      {
        "foreground": "005cc5",
        "token": "meta.property-name"
      },
      {
        "foreground": "e36209",
        "token": "variable"
      },
      {
        "foreground": "24292e",
        "token": "variable.other"
      },
      {
        "foreground": "b31d28",
        "fontStyle": "bold italic underline",
        "token": "invalid.broken"
      },
      {
        "foreground": "b31d28",
        "fontStyle": "bold italic underline",
        "token": "invalid.deprecated"
      },
      {
        "foreground": "fafbfc",
        "background": "b31d28",
        "fontStyle": "italic underline",
        "token": "invalid.illegal"
      },
      {
        "foreground": "fafbfc",
        "background": "d73a49",
        "fontStyle": "italic underline",
        "token": "carriage-return"
      },
      {
        "foreground": "b31d28",
        "fontStyle": "bold italic underline",
        "token": "invalid.unimplemented"
      },
      {
        "foreground": "b31d28",
        "token": "message.error"
      },
      {
        "foreground": "24292e",
        "token": "string source"
      },
      {
        "foreground": "005cc5",
        "token": "string variable"
      },
      {
        "foreground": "032f62",
        "token": "source.regexp"
      },
      {
        "foreground": "032f62",
        "token": "string.regexp"
      },
      {
        "foreground": "032f62",
        "token": "string.regexp.character-class"
      },
      {
        "foreground": "032f62",
        "token": "string.regexp constant.character.escape"
      },
      {
        "foreground": "032f62",
        "token": "string.regexp source.ruby.embedded"
      },
      {
        "foreground": "032f62",
        "token": "string.regexp string.regexp.arbitrary-repitition"
      },
      {
        "foreground": "22863a",
        "fontStyle": "bold",
        "token": "string.regexp constant.character.escape"
      },
      {
        "foreground": "005cc5",
        "token": "support.constant"
      },
      {
        "foreground": "005cc5",
        "token": "support.variable"
      },
      {
        "foreground": "005cc5",
        "token": "meta.module-reference"
      },
      {
        "foreground": "735c0f",
        "token": "markup.list"
      },
      {
        "foreground": "005cc5",
        "fontStyle": "bold",
        "token": "markup.heading"
      },
      {
        "foreground": "005cc5",
        "fontStyle": "bold",
        "token": "markup.heading entity.name"
      },
      {
        "foreground": "22863a",
        "token": "markup.quote"
      },
      {
        "foreground": "24292e",
        "fontStyle": "italic",
        "token": "markup.italic"
      },
      {
        "foreground": "24292e",
        "fontStyle": "bold",
        "token": "markup.bold"
      },
      {
        "foreground": "005cc5",
        "token": "markup.raw"
      },
      {
        "foreground": "b31d28",
        "background": "ffeef0",
        "token": "markup.deleted"
      },
      {
        "foreground": "b31d28",
        "background": "ffeef0",
        "token": "meta.diff.header.from-file"
      },
      {
        "foreground": "b31d28",
        "background": "ffeef0",
        "token": "punctuation.definition.deleted"
      },
      {
        "foreground": "22863a",
        "background": "f0fff4",
        "token": "markup.inserted"
      },
      {
        "foreground": "22863a",
        "background": "f0fff4",
        "token": "meta.diff.header.to-file"
      },
      {
        "foreground": "22863a",
        "background": "f0fff4",
        "token": "punctuation.definition.inserted"
      },
      {
        "foreground": "e36209",
        "background": "ffebda",
        "token": "markup.changed"
      },
      {
        "foreground": "e36209",
        "background": "ffebda",
        "token": "punctuation.definition.changed"
      },
      {
        "foreground": "f6f8fa",
        "background": "005cc5",
        "token": "markup.ignored"
      },
      {
        "foreground": "f6f8fa",
        "background": "005cc5",
        "token": "markup.untracked"
      },
      {
        "foreground": "6f42c1",
        "fontStyle": "bold",
        "token": "meta.diff.range"
      },
      {
        "foreground": "005cc5",
        "token": "meta.diff.header"
      },
      {
        "foreground": "005cc5",
        "fontStyle": "bold",
        "token": "meta.separator"
      },
      {
        "foreground": "005cc5",
        "token": "meta.output"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.tag"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.curly"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.round"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.square"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.angle"
      },
      {
        "foreground": "586069",
        "token": "brackethighlighter.quote"
      },
      {
        "foreground": "b31d28",
        "token": "brackethighlighter.unmatched"
      },
      {
        "foreground": "b31d28",
        "token": "sublimelinter.mark.error"
      },
      {
        "foreground": "e36209",
        "token": "sublimelinter.mark.warning"
      },
      {
        "foreground": "959da5",
        "token": "sublimelinter.gutter-mark"
      },
      {
        "foreground": "032f62",
        "fontStyle": "underline",
        "token": "constant.other.reference.link"
      },
      {
        "foreground": "032f62",
        "fontStyle": "underline",
        "token": "string.other.link"
      }
    ],
    "colors": {
      "editor.foreground": "#24292e",
      "editor.background": "#ffffff",
      "editor.selectionBackground": "#c8c8fa",
      "editor.inactiveSelectionBackground": "#fafbfc",
      "editor.lineHighlightBackground": "#fafbfc",
      "editorCursor.foreground": "#24292e",
      "editorWhitespace.foreground": "#959da5",
      "editorIndentGuide.background": "#959da5",
      "editorIndentGuide.activeBackground": "#24292e",
      "editor.selectionHighlightBorder": "#fafbfc"
    }
  };
