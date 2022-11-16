import init, { run_vrl } from "./vrl_web_playground.js";
init().then(() => {
    window.run_vrl = run_vrl;
    // require is provided by loader.min.js.
      require.config({
        paths: { vs: "https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.26.1/min/vs" },
      });
      require(["vs/editor/editor.main"], () => {
        window.programEditor = monaco.editor.create(document.getElementById("container-program"), {
          value: `# Remove some fields
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
}`,
          language: "coffeescript",
          theme: "vs-light",
          minimap: { enabled: false },
          automaticLayout: true,
        });

        window.eventEditor = monaco.editor.create(document.getElementById("container-event"), {
          value: `{
	"message": "Hello VRL",
	"foo": "delete me",
	"http_status": "200"
}`,
          language: "json",
          theme: "vs-light",
          minimap: { enabled: false },
          automaticLayout: true,
        });

        window.outputEditor = monaco.editor.create(document.getElementById("container-output"), {
          language: "json",
          theme: "vs-light",
          minimap: { enabled: false },
          automaticLayout: true,
        });
        const queryString = window.location.search;
        if (queryString.length != 0) {
          const urlParams = new URLSearchParams(queryString);
          const stateParam = decodeURIComponent(urlParams.get("state"));

          try {
            let urlState = JSON.parse(atob(stateParam));

            window.programEditor.setValue(urlState["program"]);
            window.eventEditor.setValue(JSON.stringify(urlState["event"], null, "\t"));

            console.log("[DEBUG::queryStringLogic] Current Params:", JSON.parse(atob(stateParam)));
            let res = handleRunCode(JSON.parse(atob(stateParam)));
            console.log("[DEBUG::queryStringLogic] Running VRL with current Params:", res);
          } catch (e) {
            setNormalEditor(`Error reading the shared URL\n${e}`);
          }
        }
      });
      function tryJsonParse(str) {
        try {
          return JSON.parse(str);
        } catch (e) {
          monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: false
          });
          setNormalEditor(`Error attempting to parse the following string into valid JSON\n
String: ${str}
\nEnsure that the Event editor contains valid JSON
\nCommon mistakes:\n
  Trailing Commas\n  Last line is a newline or whitespace\n  Unbalanced curly braces
  If using JSONL, ensure each line is valid JSON`);
        }
      }

      function setDiffEditor(original, modified) {
        // is the current output editor a normal editor?
        // if so dispose it, and create a diff editor instead
        if (window.outputEditor.setValue != undefined) {
            window.outputEditor.dispose();
            window.outputEditor = monaco.editor.createDiffEditor(document.getElementById('container-output'),{
                minimap: {enabled: false},
                automaticLayout: true,
                scrollbar: {vertical: "hidden"}
            });
        }
        
          let originalModel = monaco.editor.createModel(JSON.stringify(original, null, "\t"));
          let modifiedModel = monaco.editor.createModel(JSON.stringify(modified, null, "\t"));

          
          window.outputEditor.setModel({
            original: originalModel,
            modified: modifiedModel
          });

          window.outputEditorDiffNavigator = monaco.editor.createDiffNavigator(window.outputEditor, {
            followsCarret: true,
            ignoreCharChanges: true
          });

          window.outputEditorDiffNavigatorIntervalId = window.setInterval(function() {
            window.outputEditorDiffNavigator.next();
          }, 2000);
      }

      function setNormalEditor(output) {
        // if the current output editor is a diff editor,
        // dispose of it and create a normal editor
        if (window.outputEditor.setModel != undefined) {
            window.outputEditor.dispose();
            if (window.outputEditorDiffNavigator) {
                window.outputEditorDiffNavigator.dispose();
            }
            
            window.clearInterval(window.outputEditorDiffNavigatorIntervalId);
            window.outputEditor = monaco.editor.create(document.getElementById("container-output"), {
                value: output,
                language: "json",
                theme: "vs-light",
                minimap: { enabled: false },
                automaticLayout: true,
              });
        }
        window.outputEditor.setValue(output);
      }

      function isJsonL() {
        if (window.eventEditor.getModel().getLineCount() > 1) {
          let lines = window.eventEditor.getModel().getLinesContent();
          // if the second line is a json object
          // we assume the user has passed in valid json on each
          // line
          if (lines[1][0] == "{" && lines[1][lines[1].length - 1] == "}") {
            return true;
          }
        }
        return false;
      }
      window.handleRunCode = function handleRunCode(input) {
        if (isJsonL()) {
          let inputs = [];
          let program = window.programEditor.getValue();
          let lines = window.eventEditor.getModel().getLinesContent();
          lines.forEach((line) => {
            inputs.push({
              program: program,
              event: tryJsonParse(line)
            })
          });

          let results = [];
          inputs.forEach((input) => {
            results.push(window.run_vrl(input));
          })
          let outputs = [];
          results.forEach((res) => {
            if (res.output) {
              outputs.push(JSON.stringify(res["result"], null, "\t"));
            } else if (res.msg) {
              outputs.push(res["msg"]);
            }
          })
          // disable output validation for json since jsonl input won't ouput valid json
          monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: false
          });
          setNormalEditor(outputs.join("\n"));
          return results;
        }

        if (input == null) {
          input = {
            program: window.programEditor.getValue(),
            event: tryJsonParse(window.eventEditor.getValue()),
          };
        }

        let res = window.run_vrl(input);

        console.log("[DEBUG::handleRunCode()] Printing out res: ", res);
        if (res.output) {
          // implement diff view here
          setDiffEditor(input.event, res["result"]);
          

        } else if (res.msg) {
          // disable output validation for json
          // since vrl error msgs won't ouput json
          monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: false
          });
          setNormalEditor(res["msg"]);
        }
        return res;
      }
      window.handleShareCode = function handleShareCode() {
        let state = {
          program: window.programEditor.getValue(),
          event: JSON.parse(window.eventEditor.getValue()),
        };

        console.log("[DEBUG::handleShareCode()] Printing out state", state);
        console.log(
          "[DEBUG::handleShareCode()] Printing out base64 encoded state\n",
          btoa(JSON.stringify(state))
        );
        window.history.pushState(state, "", `?state=${encodeURIComponent(btoa(JSON.stringify(state)))}`);
      }
});