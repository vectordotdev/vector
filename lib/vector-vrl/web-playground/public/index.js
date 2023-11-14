import init, { run_vrl, vrl_version, vrl_link, vector_version, vector_link } from "./pkg/vector_vrl_web_playground.js";
import { vrlLanguageDefinition, vrlThemeDefinition } from "./vrl-highlighter.js";

const PROGRAM_EDITOR_DEFAULT_VALUE = `# Remove some fields
del(.foo)

# Add a timestamp
.timestamp = now()

# Parse HTTP status code into local variable
http_status_code = parse_int!(.http_status)
del(.http_status)

# Add status
if http_status_code >= 200 && http_status_code <= 299 {
    .status = "success"
} else {
    .status = "error"
}`;

const EVENT_EDITOR_DEFAULT_VALUE = `{
    "message": "Hello VRL",
    "foo": "delete me",
    "http_status": "200"
}`;

const OUTPUT_EDITOR_DEFAULT_VALUE = `{}`;

const ERROR_INVALID_JSONL_EVENT_MSG = `Error attempting to parse the following string into valid JSON\n
String: {{str}}
\nEnsure that the Event editor contains valid JSON
\nCommon mistakes:\n
  Trailing Commas\n  Last line is a newline or whitespace\n  Unbalanced curly braces
  If using JSONL (one log per line), ensure each line is valid JSON\n
You can try validating your JSON here: https://jsonlint.com/ \n`;

export class VrlWebPlayground {

    constructor() {
        let temp = init().then(() => {
            this.run_vrl = run_vrl;

            this.vector_version = vector_version();
            this.vector_link = vector_link();

            this.vrl_version = vrl_version();
            this.vrl_link = vrl_link();

            // require is provided by loader.min.js.
            require.config({
                paths: { vs: "https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.26.1/min/vs" },
            });
            // monaco and run_vrl will exist only inside this block
            // due to http requests
            // TODO: refactor function to be async and await on monaco and run_vrl
            require(["vs/editor/editor.main"], () => {
                this.monaco = monaco;
                // set up vrl highlighting
                this.monaco.languages.register( {id: 'vrl'} );
                // Register a tokens provider for the language
                this.monaco.editor.defineTheme('vrl-theme', vrlThemeDefinition);
                this.monaco.languages.setMonarchTokensProvider('vrl', vrlLanguageDefinition);
                this.eventEditor = this.createDefaultEditor("container-event", EVENT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
                this.outputEditor = this.createDefaultEditor("container-output", OUTPUT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
                this.programEditor = this.createDefaultEditor("container-program", PROGRAM_EDITOR_DEFAULT_VALUE, "vrl", "vrl-theme");


                const queryString = window.location.search;
                if (queryString.length != 0) {
                    const urlParams = new URLSearchParams(queryString);
                    const stateParam = decodeURIComponent(urlParams.get("state"));

                    try {
                        let urlState = JSON.parse(atob(stateParam));

                        this.programEditor.setValue(urlState["program"]);

                        if (urlState["is_jsonl"] == true) {
                            this.eventEditor.setValue(urlState["event"]);
                        } else {
                            this.eventEditor.setValue(JSON.stringify(urlState["event"], null, "\t"));
                        }

                        console.log("[DEBUG::queryStringLogic] Current Params:", JSON.parse(atob(stateParam)));
                        let res = this.handleRunCode(JSON.parse(atob(stateParam)));
                        console.log("[DEBUG::queryStringLogic] Running VRL with current Params:", res);
                    } catch (e) {
                        this.outputEditor.setValue(`Error reading the shared URL\n${e}`);
                    }
                }

                this.addVersions();
            });
        });
    }

    addVersions() {
        let vectorLinkElement = document.getElementById('vector-version-link');
        vectorLinkElement.text = this.vector_version.substring(0, 8);
        vectorLinkElement.href = this.vector_link;

        let vrlLinkElement = document.getElementById('vrl-version-link');
        vrlLinkElement.text = this.vrl_version.substring(0, 8);
        vrlLinkElement.href = this.vrl_link;
    }

    createDefaultEditor(elementId, value, language, theme) {
        return this.monaco.editor.create(document.getElementById(elementId), {
            value: value,
            language: language,
            theme: theme,
            minimap: { enabled: false },
            automaticLayout: true,
        });
    }

    getState() {
        if (this.eventEditorIsJsonl()) {
            return {
                program: this.programEditor.getValue(),
                event: this.eventEditor.getModel().getLinesContent().join("\n"),
                is_jsonl: true,
                error: null,
            };
        }

        const editorValue = this.eventEditor.getValue();
        try {
            return {
                program: this.programEditor.getValue(),
                event: JSON.parse((editorValue.length === 0) ? "{}" : editorValue),
                is_jsonl: false,
                error: null,
            };
        }
        catch (error) {
            console.error(error);
            return {
                program: this.programEditor.getValue(),
                event: null,
                is_jsonl: false,
                error: `Could not parse JSON event:\n${editorValue}`,
            };
        }
        return state;
    }

    disableJsonLinting() {
        this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: false
        });
    }

    enableJsonLinting() {
        this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: true
        });
    }

    tryJsonParse(str) {
        try {
            return JSON.parse(str);
        } catch (e) {
            this.disableJsonLinting();
            let err = ERROR_INVALID_JSONL_EVENT_MSG.toString().replace("{{str}}", str);
            this.outputEditor.setValue(err);
            throw new Error(err);
        }
    }

    eventEditorIsJsonl() {
        if (this.eventEditor.getModel().getLineCount() > 1) {
            let lines = this.eventEditor.getModel().getLinesContent();

            // if the second line is a json object
            // we assume the user is attempting to pass in JSONL
            // in the event editor
            if (lines[1][0] == "{" && lines[1][lines[1].length - 1] == "}") {
                return true;
            }

            return false;
        }
    }

    /**
     *
     * @param {object} input
     *
     * input param is optional
     * input param is mainly used when we are parsing the
     * url for state parameters (when a user shared their program)
     *
     * {
     *     program: str,
     *     event: object
     * }
     */
    handleRunCode(input) {
        if (this.eventEditorIsJsonl()) {
            return this.handleRunCodeJsonl();
        }

        if (input == null) {
            input = this.getState();
        }
        if (input.error) {
            this.disableJsonLinting();
            this.outputEditor.setValue(input.error);
            return input;
        }

        let res = this.run_vrl(input);
        console.log("[DEBUG::handleRunCode()] Printing out res: ", res);
        if (res.result) {
            this.outputEditor.setValue(JSON.stringify(res.result, null, "\t"));
        } else if (res.msg) {
            // disable json linting for error msgs
            // since error msgs are not valid json
            this.disableJsonLinting();
            this.outputEditor.setValue(res.msg);
        }
        return res;
    }

    handleRunCodeJsonl() {
        let inputs = [];
        let program = this.programEditor.getValue();
        let eventLines = this.eventEditor.getModel().getLinesContent();

        // each line in eventLines is a json object
        // we will use the same program in the program editor
        // and run it against each line
        eventLines.forEach((line) => {
            inputs.push({
                program: program,
                event: this.tryJsonParse(line)
            })
        });

        let results = [];
        inputs.forEach((input) => {
            results.push(this.run_vrl(input));
        })

        let outputs = [];
        results.forEach((res) => {
            if (res.output != null) {
                outputs.push(JSON.stringify(res["result"], null, "\t"));
            } else if (res.msg != null) {
                outputs.push(res["msg"]);
            }
        });
        this.disableJsonLinting();
        this.outputEditor.setValue(outputs.join("\n"));
        return results;
    }

    handleShareCode() {
        let state = this.getState();
        console.log("[DEBUG::handleShareCode()] Printing out state", state);
        console.log(
          "[DEBUG::handleShareCode()] Printing out base64 encoded state\n",
          btoa(JSON.stringify(state))
        );
        window.history.pushState(state, "", `?state=${encodeURIComponent(btoa(JSON.stringify(state)))}`);
    }
}

window.vrlPlayground = new VrlWebPlayground();
