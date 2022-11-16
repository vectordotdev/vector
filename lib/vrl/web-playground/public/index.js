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
            window.outputEditor.setValue(`Error reading the shared URL\n${e}`);
          }
        }
      });
});
