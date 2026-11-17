// Hugo finishes before the external highlighter. Delay full reloads until the
// requested page contains the completed VRL markup.
window.addEventListener("load", () => {
  if (!window.LiveReload) return;

  const unhighlighted = 'code.language-vrl:not([data-highlighter="syntect-vrl"])';
  let pending = false;
  async function reloadWhenHighlighted() {
    if (pending) return;
    pending = true;
    for (;;) {
      try {
        const response = await fetch(window.location.href, { cache: "no-store" });
        const document = new DOMParser().parseFromString(await response.text(), "text/html");
        if (!response.ok || !document.querySelector(unhighlighted)) {
          window.location.reload();
          return;
        }
      } catch {
        // Keep waiting while the development server restarts.
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }

  class VRLReload {
    static identifier = "vector-vrl";
    static version = "1.0";

    reload(path, options) {
      if (options.liveCSS && /\.css(?:\.map)?$/i.test(path)) return false;
      if (options.liveImg && /\.(?:png|jpe?g|gif|svg|webp|avif|ico)$/i.test(path)) return false;
      void reloadWhenHighlighted();
      return true;
    }
  }
  window.LiveReload.addPlugin(VRLReload);
  // Also cover the first visit while the initial highlighter pass is running.
  if (document.querySelector(unhighlighted)) void reloadWhenHighlighted();
});
