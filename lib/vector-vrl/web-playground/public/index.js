import init, { run_vrl, vector_link, vector_version, vrl_link, vrl_version } from "./pkg/vector_vrl_web_playground.js";
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

const HISTORY_STORAGE_KEY = "vrl_playground_history";
const HISTORY_MAX_ENTRIES = 50;
const THEME_STORAGE_KEY = "vrl_playground_theme";

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
      paths: { vs: "https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.26.1/min/vs" }
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
    this.monaco.languages.register({ id: "vrl" });
    this.monaco.editor.defineTheme("vrl-theme", vrlThemeDefinition);
    this.monaco.editor.defineTheme("vrl-theme-dark", {
      base: "vs-dark",
      inherit: true,
      // Rely on vs-dark's default token colors — light-theme colors in vrlThemeDefinition
      // are too dark for a dark background.
      rules: [],
      colors: {
        "editor.background": "#0d1117",
        "editor.foreground": "#c9d1d9",
        "editor.lineHighlightBackground": "#161b22",
        "editorCursor.foreground": "#c9d1d9"
      }
    });
    this.monaco.languages.setMonarchTokensProvider("vrl", vrlLanguageDefinition);

    // Editors
    this.eventEditor = this.createDefaultEditor("container-event", EVENT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
    this.outputEditor = this.createDefaultEditor("container-output", OUTPUT_EDITOR_DEFAULT_VALUE, "json", "vs-light");
    this.programEditor = this.createDefaultEditor(
      "container-program",
      PROGRAM_EDITOR_DEFAULT_VALUE,
      "vrl",
      "vrl-theme"
    );

    // Resizable panels
    this._initSplitPanels();

    // Theme
    this._initTheme();

    // Versions
    this.addVersions();

    // Keyboard shortcut: Shift+Enter runs the program
    window.addEventListener("keydown", (e) => {
      if (e.shiftKey && e.key === "Enter") {
        e.preventDefault();
        this.handleRunCode();
      }
    });

    // Populate history dropdown from localStorage
    this._populateHistoryDropdown();

    const historySelect = document.getElementById("history-select");
    if (historySelect) {
      historySelect.addEventListener("change", (e) => {
        const idx = parseInt(e.target.value, 10);
        this._restoreFromHistory(idx);
        e.target.value = "";
      });
    }

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

  _initTheme() {
    let stored = null;
    try {
      stored = localStorage.getItem(THEME_STORAGE_KEY);
    } catch (_e) {
      /* storage blocked (private mode, quotas) — fall back to system */
    }
    this._themePreference = stored === "light" || stored === "dark" ? stored : "system";

    const btn = document.getElementById("theme-toggle-btn");
    if (btn) {
      btn.addEventListener("click", () => {
        const cycle = { system: "light", light: "dark", dark: "system" };
        this._themePreference = cycle[this._themePreference] || "system";
        try {
          if (this._themePreference === "system") localStorage.removeItem(THEME_STORAGE_KEY);
          else localStorage.setItem(THEME_STORAGE_KEY, this._themePreference);
        } catch (_e) {
          /* ignore storage errors — in-memory preference still applies */
        }
        this._applyTheme();
      });
    }

    // Follow system changes when in "system" mode
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    mql.addEventListener?.("change", () => {
      if (this._themePreference === "system") this._applyTheme();
    });

    this._applyTheme();
  }

  _applyTheme() {
    const preference = this._themePreference || "system";
    const isDark =
      preference === "dark" || (preference === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);

    document.documentElement.setAttribute("data-theme", isDark ? "dark" : "light");
    // Monaco's theme is global — all editors switch together
    this.monaco?.editor?.setTheme(isDark ? "vrl-theme-dark" : "vrl-theme");

    this._renderThemeButton(preference);
  }

  _renderThemeButton(preference) {
    const btn = document.getElementById("theme-toggle-btn");
    if (!btn) return;
    const icons = {
      system:
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12v11.25a2.25 2.25 0 01-2.25 2.25H5.25a2.25 2.25 0 01-2.25-2.25V5.25A2.25 2.25 0 015.25 3h13.5A2.25 2.25 0 0121 5.25z"/></svg>',
      light:
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z"/></svg>',
      dark: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.72 9.72 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z"/></svg>'
    };
    btn.innerHTML = icons[preference] || icons.system;
    btn.title = `Theme: ${preference.charAt(0).toUpperCase()}${preference.slice(1)} (click to cycle)`;
  }

  _initSplitPanels() {
    if (typeof window.Split !== "function") return;

    window.Split(["#input-section", "#output-section"], {
      sizes: [50, 50],
      minSize: 200,
      gutterSize: 6,
      cursor: "col-resize"
    });

    window.Split(["#event-cell", "#output-cell"], {
      direction: "vertical",
      sizes: [50, 50],
      minSize: 100,
      gutterSize: 6,
      cursor: "row-resize"
    });
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
      minimap: { enabled: false },
      automaticLayout: true,
      wordWrap: "on",
      scrollBeyondLastLine: false,
      scrollbar: {
        vertical: "auto",
        horizontal: "auto",
        useShadows: false
      }
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
      const text = isJson ? JSON.stringify(runResult.target_value, null, "\t") : String(runResult.target_value);
      return { text, isJson };
    }
    if (runResult?.msg != null) {
      return { text: String(runResult.msg), isJson: false };
    }
    return { text: "Error - VRL did not return a result.", isJson: false };
  }

  _setElapsed(elapsed_time) {
    const elapsedEl = document.getElementById("elapsed-time");
    if (elapsedEl && elapsed_time != null) {
      const ms = elapsed_time.toFixed(4);
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
        error: null
      };
    }

    const editorValue = this._safeGet(this.eventEditor);
    try {
      return {
        program: this._safeGet(this.programEditor),
        event: JSON.parse(editorValue.length === 0 ? "{}" : editorValue),
        is_jsonl: false,
        error: null
      };
    } catch (_err) {
      return {
        program: this._safeGet(this.programEditor),
        event: null,
        is_jsonl: false,
        error: `Could not parse JSON event:\n${editorValue}`
      };
    }
  }

  disableJsonLinting() {
    this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: false });
  }

  enableJsonLinting() {
    this.monaco.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: true });
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

  _historyLabel(entry) {
    const program = typeof entry.program === "string" ? entry.program : "";
    const firstLine = program
      .split("\n")
      .map((l) => l.trim())
      .find((l) => l.length > 0 && !l.startsWith("#"));
    const snippet = (firstLine || program.trim()).substring(0, 40);
    const when = typeof entry.timestamp === "number" ? new Date(entry.timestamp) : new Date();
    const ts = when.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    return `${ts} — ${snippet}`;
  }

  _readHistory() {
    let raw;
    try {
      raw = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) || "[]");
    } catch (_e) {
      return [];
    }
    if (!Array.isArray(raw)) return [];
    return raw.filter((e) => e && typeof e === "object" && typeof e.program === "string");
  }

  _saveToHistory(entry) {
    const history = this._readHistory();

    history.unshift({ ...entry, timestamp: Date.now() });
    if (history.length > HISTORY_MAX_ENTRIES) history.length = HISTORY_MAX_ENTRIES;

    try {
      localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history));
    } catch (_e) {
      /* ignore quota errors */
    }

    this._populateHistoryDropdown();
  }

  _populateHistoryDropdown() {
    const select = document.getElementById("history-select");
    if (!select) return;

    const history = this._readHistory();

    // Remove all options except the placeholder
    while (select.options.length > 1) select.remove(1);

    history.forEach((entry, idx) => {
      const opt = document.createElement("option");
      opt.value = idx;
      opt.textContent = this._historyLabel(entry);
      select.appendChild(opt);
    });
  }

  _restoreFromHistory(idx) {
    const history = this._readHistory();
    const entry = history[idx];
    if (!entry) return;

    this.programEditor?.setValue(entry.program);
    this.eventEditor?.setValue(entry.event ?? "");

    const tzEl = document.getElementById("timezone-input");
    if (tzEl && typeof entry.timezone === "string") {
      tzEl.value = entry.timezone === "Default" ? "" : entry.timezone;
    }

    if (entry.outputIsJson) this.enableJsonLinting();
    else this.disableJsonLinting();
    this.outputEditor?.setValue(entry.output ?? "");
    this._setElapsed(entry.elapsedTime);
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

    const { text, isJson } = this._formatRunResult(runResult);
    if (isJson) this.enableJsonLinting();
    else this.disableJsonLinting();
    this.outputEditor.setValue(text);

    this._setElapsed(runResult?.elapsed_time);
    this._saveToHistory({
      program: this._safeGet(this.programEditor),
      event: this._safeGet(this.eventEditor),
      output: text,
      outputIsJson: isJson,
      elapsedTime: runResult?.elapsed_time,
      timezone
    });
    return runResult;
  }

  handleRunCodeJsonl() {
    this._clearOutput();

    const program = this._safeGet(this.programEditor);
    const model = this.eventEditor?.getModel?.();
    const rawLines = model ? model.getLinesContent() : [];
    const lines = rawLines.map((l) => l.trim()).filter((l) => l.length > 0);

    const timezone = this._getTimezoneOrDefault();

    // Build inputs while validating JSON per line
    const inputs = lines.map((line) => ({
      program,
      event: this.tryJsonParse(line),
      is_jsonl: true
    }));

    // Run and collect results
    const results = inputs.map((input) => this.run_vrl(input, timezone));

    const outputs = results.map((r) => this._formatRunResult(r).text);
    const outputText = outputs.join("\n");

    // Output is not pure JSON (multiple objects / possible errors)
    this.disableJsonLinting();
    this.outputEditor.setValue(outputText);

    // Aggregate elapsed time, rounded
    const total = results.reduce((sum, r) => sum + (typeof r?.elapsed_time === "number" ? r.elapsed_time : 0), 0);
    this._setElapsed(total);
    this._saveToHistory({
      program,
      event: this._safeGet(this.eventEditor),
      output: outputText,
      outputIsJson: false,
      elapsedTime: total,
      timezone
    });

    return results;
  }

  handleClearHistory() {
    if (!window.confirm("Clear all run history stored in this browser?")) return;
    try {
      localStorage.removeItem(HISTORY_STORAGE_KEY);
    } catch (_e) {
      /* ignore */
    }
    this._populateHistoryDropdown();
  }

  handleShareCode() {
    const state = this.getState();
    try {
      const encoded = encodeURIComponent(btoa(JSON.stringify(state)));
      window.history.pushState(state, "", `?state=${encoded}`);
      const shareUrl = `${window.location.origin}${window.location.pathname}?state=${encoded}`;
      if (navigator.clipboard?.writeText) {
        navigator.clipboard
          .writeText(shareUrl)
          .then(() => this._flashShareButton("Copied!"))
          .catch(() => this._flashShareButton("Copy failed"));
      } else {
        this._flashShareButton("Copy unavailable");
      }
      return true;
    } catch (e) {
      this.disableJsonLinting();
      this.outputEditor.setValue(`Error encoding state for URL\n${e}`);
      return false;
    }
  }

  _flashShareButton(text) {
    const btn = document.getElementById("share-code-btn");
    if (!btn) return;
    const label = btn.querySelector("span");
    if (!label) return;
    const original = label.textContent;
    label.textContent = text;
    btn.disabled = true;
    setTimeout(() => {
      label.textContent = original;
      btn.disabled = false;
    }, 1200);
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
