// Completion, signature help and hover for the VRL program editor. Everything is built from
// the function documentation the wasm module exposes (`vrl_functions()`), so it always matches
// the functions the playground compiles.

const CALL_NAME = /([a-zA-Z_]\w*)!?\s*$/;
const NAMED_ARGUMENT = /^\s*([a-zA-Z_]\w*)\s*:(?!:)/;
// The parent path of the segment being typed: `.` for `.mes`, `.http.` for `.http.sta`.
const FIELD_PATH_BEFORE_CURSOR = /(?:^|[^\w\])}"'])(\.(?:[a-zA-Z_@][\w@]*\.)*)[\w@]*$/;
const FIELD_PATH = /(?<![\w\])}"'])\.[a-zA-Z_@][\w@]*(?:\.[a-zA-Z_@][\w@]*)*/g;
const IDENTIFIER = /^[a-zA-Z_]\w*$/;
const ASSIGNMENT = /^\s*([a-zA-Z_]\w*(?:\s*,\s*[a-zA-Z_]\w*)*)\s*=(?!=)/;
const CLOSURE_PARAMETERS = /->\s*\|([^|]*)\|/g;

// Walks the program up to the cursor, so calls, strings and comments spanning several lines
// are seen whole.
function scanToCursor(text) {
  const brackets = [];
  let quote;
  let stringStart = 0;
  let inComment = false;

  for (let index = 0; index < text.length; index++) {
    const char = text[index];

    if (inComment) {
      if (char === "\n") {
        inComment = false;
      }
      continue;
    }

    if (quote) {
      if (char === "\\") {
        index++;
      } else if (char === quote) {
        quote = undefined;
      }
      continue;
    }

    switch (char) {
      case "#":
        inComment = true;
        break;
      case '"':
      case "'":
        quote = char;
        stringStart = index + 1;
        break;
      case "(":
      case "[":
      case "{": {
        const call = char === "(" ? CALL_NAME.exec(text.slice(0, index))?.[1] : undefined;
        brackets.push({ char, call, argumentIndex: 0, argumentStart: index + 1 });
        break;
      }
      case ")":
      case "]":
      case "}":
        brackets.pop();
        break;
      case ",": {
        const innermost = brackets[brackets.length - 1];
        if (innermost) {
          innermost.argumentIndex++;
          innermost.argumentStart = index + 1;
        }
        break;
      }
    }
  }

  return { brackets, quote, stringStart, inComment };
}

function innermostCall(brackets) {
  return [...brackets].reverse().find((bracket) => bracket.call !== undefined);
}

function textBefore(model, position) {
  return model.getValueInRange({
    startLineNumber: 1,
    startColumn: 1,
    endLineNumber: position.lineNumber,
    endColumn: position.column
  });
}

// The argument the cursor is in: named (`unit: "s"`) or by position.
function activeArgumentIndex(func, call, text) {
  const named = NAMED_ARGUMENT.exec(text.slice(call.argumentStart))?.[1];
  const namedIndex = func.arguments.findIndex((argument) => argument.name === named);
  if (namedIndex >= 0) {
    return namedIndex;
  }
  return Math.min(call.argumentIndex, func.arguments.length - 1);
}

function isFallible(func) {
  return (func.internal_failure_reasons ?? []).length > 0;
}

function typeOf(argument) {
  return argument.type.join(" | ");
}

function describeArguments(func) {
  if (func.arguments.length === 0) {
    return "No arguments";
  }

  return func.arguments
    .map((argument) => {
      const required = argument.required ? "*(required)*" : "*(optional)*";
      let details = "";
      if (argument.default !== undefined) {
        details += ` — default: \`${argument.default}\``;
      }
      if (argument.enum) {
        const values = Object.keys(argument.enum).map((value) => `\`${value}\``);
        details += ` — one of ${values.join(", ")}`;
      }
      return `- **${argument.name}** ${required}: \`${typeOf(argument)}\`${details}\n  ${argument.description}`;
    })
    .join("\n");
}

function describeFunction(func) {
  const sections = [`**${func.name}** · ${func.category}`, func.description];
  if (isFallible(func)) {
    sections.push(`Fallible: call it as \`${func.name}!(…)\` to abort on error, or handle the error.`);
  }
  sections.push(...(func.notices ?? []));
  sections.push(`**Arguments:**\n${describeArguments(func)}`);
  sections.push(`**Returns:** \`${func.return.types.join(" | ")}\``);
  const example = func.examples?.[0];
  if (example) {
    sections.push(`**Example:** ${example.title}\n\`\`\`vrl\n${example.source}\n\`\`\``);
  }
  return sections.join("\n\n");
}

function parameterLabel(argument) {
  return `${argument.name}${argument.required ? "" : "?"}: ${typeOf(argument)}`;
}

function signatureOf(func) {
  return `${func.name}(${func.arguments.map(parameterLabel).join(", ")})`;
}

function placeholderFor(argument) {
  const firstValue = argument.enum ? Object.keys(argument.enum)[0] : undefined;
  if (firstValue !== undefined) {
    return `"${firstValue}"`;
  }
  if (argument.type.includes("string")) {
    return `"${argument.name}"`;
  }
  if (argument.type.includes("array")) {
    return "[]";
  }
  if (argument.type.includes("boolean")) {
    return "false";
  }
  if (argument.type.includes("integer")) {
    return "0";
  }
  return argument.name;
}

function snippetFor(callName, func) {
  const required = func.arguments.filter((argument) => argument.required);
  const parameters = required.map((argument, index) => `\${${index + 1}:${placeholderFor(argument)}}`);
  return `${callName}(${parameters.join(", ")})`;
}

function quoteSegment(name) {
  return IDENTIFIER.test(name) ? name : JSON.stringify(name);
}

// The keys of the sample event under `parentPath`, e.g. `.http.` lists the keys of `event.http`.
function eventFieldsUnder(event, parentPath) {
  let value = event;
  for (const segment of parentPath.split(".").filter((part) => part.length > 0)) {
    value = value?.[segment];
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return [];
  }
  return Object.keys(value);
}

export function registerVrlCompletion(monaco, functionDocs, { languageId, keywords, getSampleEvent }) {
  const functions = new Map(functionDocs.map((func) => [func.name, func]));
  const keywordSet = new Set(keywords);
  const { CompletionItemKind, CompletionItemInsertTextRule, CompletionTriggerKind } = monaco.languages;

  function functionSuggestions(range) {
    return functionDocs.flatMap((func) => {
      const callNames = isFallible(func) ? [func.name, `${func.name}!`] : [func.name];
      return callNames.map((callName) => ({
        label: callName,
        kind: CompletionItemKind.Function,
        detail: signatureOf(func),
        documentation: { value: describeFunction(func) },
        insertText: snippetFor(callName, func),
        insertTextRules: CompletionItemInsertTextRule.InsertAsSnippet,
        range,
        sortText: `1_${callName}`
      }));
    });
  }

  function variableSuggestions(model, position, range) {
    const declared = new Map();
    const declare = (names, lineNumber) => {
      for (const name of names.split(",").map((candidate) => candidate.trim())) {
        if (IDENTIFIER.test(name) && !keywordSet.has(name) && !declared.has(name)) {
          declared.set(name, lineNumber);
        }
      }
    };

    for (let lineNumber = 1; lineNumber < position.lineNumber; lineNumber++) {
      const line = model.getLineContent(lineNumber);
      if (line.trim().startsWith("#")) {
        continue;
      }
      const assignment = ASSIGNMENT.exec(line);
      if (assignment) {
        declare(assignment[1], lineNumber);
      }
      for (const closure of line.matchAll(CLOSURE_PARAMETERS)) {
        declare(closure[1], lineNumber);
      }
    }

    return [...declared].map(([name, lineNumber]) => ({
      label: name,
      kind: CompletionItemKind.Variable,
      detail: `Local variable, line ${lineNumber}`,
      insertText: name,
      range,
      sortText: `0_${name}`
    }));
  }

  function fieldSuggestions(model, position, parentPath, range) {
    const cursorOffset = model.getOffsetAt(position);
    const fields = new Map();

    for (const name of eventFieldsUnder(getSampleEvent(), parentPath)) {
      fields.set(name, "Field of the event");
    }

    for (const match of model.getValue().matchAll(FIELD_PATH)) {
      const path = match[0];
      if (match.index + path.length === cursorOffset || !path.startsWith(parentPath)) {
        continue;
      }
      const segment = path.slice(parentPath.length).split(".")[0];
      if (segment && !fields.has(segment)) {
        fields.set(segment, "Field used in this program");
      }
    }

    return [...fields].map(([name, description]) => ({
      label: name,
      kind: CompletionItemKind.Field,
      detail: description,
      insertText: quoteSegment(name),
      range
    }));
  }

  function enumSuggestions(model, position, text, cursor) {
    const call = innermostCall(cursor.brackets);
    const func = call ? functions.get(call.call) : undefined;
    const argument = func?.arguments[activeArgumentIndex(func, call, text)];
    if (!argument?.enum) {
      return [];
    }

    const start = model.getPositionAt(cursor.stringStart);
    const range = {
      startLineNumber: start.lineNumber,
      startColumn: start.column,
      endLineNumber: position.lineNumber,
      endColumn: position.column
    };
    return Object.entries(argument.enum).map(([value, description]) => ({
      label: value,
      kind: CompletionItemKind.EnumMember,
      documentation: description,
      insertText: value,
      range
    }));
  }

  monaco.languages.registerCompletionItemProvider(languageId, {
    triggerCharacters: [".", '"', "'"],
    provideCompletionItems: (model, position, context) => {
      const text = textBefore(model, position);
      const cursor = scanToCursor(text);
      if (cursor.inComment) {
        return { suggestions: [] };
      }

      if (cursor.quote !== undefined) {
        return { suggestions: enumSuggestions(model, position, text, cursor) };
      }

      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn
      };

      const fieldPath = FIELD_PATH_BEFORE_CURSOR.exec(text);
      if (fieldPath) {
        return { suggestions: fieldSuggestions(model, position, fieldPath[1], range) };
      }

      if (context.triggerKind === CompletionTriggerKind.TriggerCharacter) {
        return { suggestions: [] };
      }

      return { suggestions: [...variableSuggestions(model, position, range), ...functionSuggestions(range)] };
    }
  });

  monaco.languages.registerSignatureHelpProvider(languageId, {
    signatureHelpTriggerCharacters: ["(", ","],
    signatureHelpRetriggerCharacters: [",", ":"],
    provideSignatureHelp: (model, position) => {
      const text = textBefore(model, position);
      const cursor = scanToCursor(text);
      const call = innermostCall(cursor.brackets);
      const func = call ? functions.get(call.call) : undefined;
      if (cursor.inComment || !func) {
        return null;
      }

      return {
        value: {
          signatures: [
            {
              label: signatureOf(func),
              documentation: { value: func.description },
              parameters: func.arguments.map((argument) => ({
                label: parameterLabel(argument),
                documentation: { value: argument.description }
              }))
            }
          ],
          activeSignature: 0,
          activeParameter: activeArgumentIndex(func, call, text)
        },
        dispose: () => {}
      };
    }
  });

  monaco.languages.registerHoverProvider(languageId, {
    provideHover: (model, position) => {
      const word = model.getWordAtPosition(position);
      const func = word ? functions.get(word.word) : undefined;
      if (!func) {
        return null;
      }

      // `.parse_json` is a field, not the function.
      const charBefore = model.getValueInRange({
        startLineNumber: position.lineNumber,
        startColumn: Math.max(1, word.startColumn - 1),
        endLineNumber: position.lineNumber,
        endColumn: word.startColumn
      });
      if (charBefore === ".") {
        return null;
      }

      return {
        range: new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn),
        contents: [{ value: describeFunction(func) }]
      };
    }
  });
}
