import init, {run_vrl, vector_link, vector_version, vrl_link, vrl_version} from "./pkg/vector_vrl_web_playground.js";
import {vrlLanguageDefinition, vrlThemeDefinition} from "./vrl-highlighter.js";

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

const ERROR_INVALID_JSONL_EVENT_MSG = `Error attempting to parse the following string into valid JSON

String: {{str}}

Ensure that the Event editor contains valid JSON

Common mistakes:
  Trailing Commas
  Last line is a newline or whitespace
  Unbalanced curly braces
  If using JSONL (one log per line), ensure each line is valid JSON

You can try validating your JSON here: https://jsonlint.com/
`;

function loadMonaco() {
    return new Promise((resolve, reject) => {
        // require is provided by loader.min.js.
        require.config({
            paths: {vs: "https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.26.1/min/vs"},
        });
        require(["vs/editor/editor.main"], () => resolve(window.monaco), reject);
    });
}

export class VrlWebPlayground {
    static async create() {
        const instance = new VrlWebPlayground(true);
        await instance._initAsync();
        return instance;
    }

    constructor(_internal = false) {
        if (!_internal) {
            // Prefer factory: VrlWebPlayground.create()
            this._initAsync(); // fire-and-forget fallback
        }
    }

    async _initAsync() {
        // Load wasm/runtime
        await init();

        // Bind native funcs/versions
        this.run_vrl = run_vrl;
        this.vector_version = vector_version();
        this.vector_link = vector_link();
        this.vrl_version = vrl_version();
        this.vrl_link = vrl_link();

        // Load Monaco
        this.monaco = await loadMonaco();

        // VRL lang + theme
        this.monaco.languages.register({id: "vrl"});
        this.monaco.editor.defineTheme("vrl-theme", vrlThemeDefinition);
        this.monaco.languages.setMonarchTokensProvider("vrl", vrlLanguageDefinition);

        // Editors
        this.eventEditor = this.createDefaultEditor("container-event", EVENT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
        this.outputEditor = this.createDefaultEditor("container-output", OUTPUT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
        this.programEditor = this.createDefaultEditor("container-program", PROGRAM_EDITOR_DEFAULT_VALUE, "vrl", "vrl-theme");

        // Versions
        this.addVersions();

        // Handle shared state from URL (if present)
        this._maybeLoadFromUrl();
    }

    _maybeLoadFromUrl() {
        const qs = window.location.search;
        if (!qs) return;

        const urlParams = new URLSearchParams(qs);
        const stateParam = urlParams.get("state");
        if (!stateParam) return;

        try {
            const decoded = atob(decodeURIComponent(stateParam));
            const urlState = JSON.parse(decoded);

            if (typeof urlState.program === "string") {
                this.programEditor.setValue(urlState.program);
            }

            if (urlState.is_jsonl === true && typeof urlState.event === "string") {
                this.eventEditor.setValue(urlState.event);
            } else if (urlState.event != null) {
                this.eventEditor.setValue(JSON.stringify(urlState.event, null, "\t"));
            }

            // Run immediately with the provided state
            this.handleRunCode(urlState);
        } catch (e) {
            this.disableJsonLinting();
            this.outputEditor.setValue(`Error reading the shared URL\n${e}`);
        }
    }

    addVersions() {
        const vectorLinkElement = document.getElementById("vector-version-link");
        if (vectorLinkElement) {
            vectorLinkElement.text = (this.vector_version || "").toString().substring(0, 8);
            vectorLinkElement.href = this.vector_link || "#";
        }

        const vrlLinkElement = document.getElementById("vrl-version-link");
        if (vrlLinkElement) {
            vrlLinkElement.text = (this.vrl_version || "").toString().substring(0, 8);
            vrlLinkElement.href = this.vrl_link || "#";
        }
    }

    createDefaultEditor(elementId, value, language, theme) {
        const el = document.getElementById(elementId);
        if (!el) {
            console.warn(`Editor container #${elementId} not found`);
            return null;
        }
        return this.monaco.editor.create(el, {
            value,
            language,
            theme,
            minimap: {enabled: false},
            automaticLayout: true,
            wordWrap: 'on',
        });
    }

    _clearOutput() {
        if (this.outputEditor) {
            // wipe the buffer so stale values never linger
            this.outputEditor.setValue("");
        }
        const elapsedEl = document.getElementById("elapsed-time");
        if (elapsedEl) {
            elapsedEl.textContent = "";
        }
    }

    _formatRunResult(runResult) {
        if (runResult?.target_value != null) {
            const isJson = typeof runResult.target_value === "object";
            const text = isJson
                ? JSON.stringify(runResult.target_value, null, "\t")
                : String(runResult.target_value);
            return {text, isJson};
        }
        if (runResult?.msg != null) {
            return {text: String(runResult.msg), isJson: false};
        }
        return {text: "Error - VRL did not return a result.", isJson: false};
    }

    _setElapsed(elapsed_time) {
        const elapsedEl = document.getElementById("elapsed-time");
        if (elapsedEl && elapsed_time != null) {
            const ms = elapsed_time.toFixed(4)
            elapsedEl.textContent = `Duration: ${ms} milliseconds`;
        }
    }

    _safeGet(editor, fallback = "") {
        return editor?.getValue?.() ?? fallback;
    }

    getState() {
        if (this.eventEditorIsJsonl()) {
            return {
                program: this._safeGet(this.programEditor),
                event: this.eventEditor.getModel().getLinesContent().join("\n"),
                is_jsonl: true,
                error: null,
            };
        }

        const editorValue = this._safeGet(this.eventEditor);
        try {
            return {
                program: this._safeGet(this.programEditor),
                event: JSON.parse(editorValue.length === 0 ? "{}" : editorValue),
                is_jsonl: false,
                error: null,
            };
        } catch (_err) {
            return {
                program: this._safeGet(this.programEditor),
                event: null,
                is_jsonl: false,
                error: `Could not parse JSON event:\n${editorValue}`,
            };
        }
    }

    disableJsonLinting() {
        this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({validate: false});
    }

    enableJsonLinting() {
        this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({validate: true});
    }

    tryJsonParse(str) {
        try {
            return JSON.parse(str);
        } catch (_e) {
            this.disableJsonLinting();
            const err = ERROR_INVALID_JSONL_EVENT_MSG.toString().replace("{{str}}", str);
            this.outputEditor.setValue(err);
            throw new Error(err);
        }
    }

    /**
     * Treat as JSONL if there are >1 non-empty lines and at least the second non-empty
     * line *appears* to be a JSON object. Robust to whitespace.
     */
    eventEditorIsJsonl() {
        const model = this.eventEditor?.getModel?.();
        if (!model) return false;

        const rawLines = model.getLinesContent();
        const lines = rawLines.map((l) => l.trim()).filter((l) => l.length > 0);
        if (lines.length <= 1) return false;

        const second = lines[1];
        return second.startsWith("{") && second.endsWith("}");
    }

    _getTimezoneOrDefault() {
        const tzEl = document.getElementById("timezone-input");
        return tzEl?.value && tzEl.value.trim().length > 0 ? tzEl.value.trim() : "Default";
    }

    handleRunCode(input) {
        this._clearOutput();

        // JSONL path short-circuit
        if (this.eventEditorIsJsonl()) {
            return this.handleRunCodeJsonl();
        }

        if (input == null) {
            input = this.getState();
        }

        if (input.error) {
            console.error(input.error);
            this.disableJsonLinting();
            this.outputEditor.setValue(input.error);
            return input;
        }

        const timezone = this._getTimezoneOrDefault();
        console.debug("Selected timezone: ", timezone);
        const runResult = this.run_vrl(input, timezone);
        console.log("Run result: ", runResult);

        const {text, isJson} = this._formatRunResult(runResult);
        if (isJson) this.enableJsonLinting(); else this.disableJsonLinting();
        this.outputEditor.setValue(text);

        this._setElapsed(runResult?.elapsed_time);
        return runResult;
    }

    handleRunCodeJsonl() {
        this._clearOutput();

        const program = this._safeGet(this.programEditor);
        const model = this.eventEditor?.getModel?.();
        const rawLines = model ? model.getLinesContent() : [];
        const lines = rawLines.map(l => l.trim()).filter(l => l.length > 0);

        const timezone = this._getTimezoneOrDefault();

        // Build inputs while validating JSON per line
        const inputs = lines.map(line => ({
            program,
            event: this.tryJsonParse(line),
            is_jsonl: true,
        }));

        // Run and collect results
        const results = inputs.map(input => this.run_vrl(input, timezone));

        const outputs = results.map(r => this._formatRunResult(r).text);

        // Output is not pure JSON (multiple objects / possible errors)
        this.disableJsonLinting();
        this.outputEditor.setValue(outputs.join("\n"));

        // Aggregate elapsed time, rounded
        const total = results.reduce((sum, r) => sum + (typeof r?.elapsed_time === "number" ? r.elapsed_time : 0), 0);
        this._setElapsed(total);

        return results;
    }

    handleShareCode() {
        const state = this.getState();
        try {
            const encoded = encodeURIComponent(btoa(JSON.stringify(state)));
            window.history.pushState(state, "", `?state=${encoded}`);
            return true;
        } catch (e) {
            this.disableJsonLinting();
            this.outputEditor.setValue(`Error encoding state for URL\n${e}`);
            return false;
        }
    }
}

// Prefer the async factory to ensure everything is loaded before use:
VrlWebPlayground.create()
    .then((instance) => {
        window.vrlPlayground = instance;
    })
    .catch((err) => {
        console.error("Failed to initialize VrlWebPlayground:", err);
    });
