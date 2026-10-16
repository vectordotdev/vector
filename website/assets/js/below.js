import tocbot from "tocbot";

// Table of contents for documentation pages
const tableOfContents = () => {
  if (document.getElementById("toc")) {
    tocbot.init({
      tocSelector: "#toc",
      contentSelector: "#page-content",
      headingSelector: "h2,h3,h4,h5",
      scrollSmoothDuration: 400,
      headingsOffset: 80,
      scrollSmoothOffset: -80,
      hasInnerContainers: true
    });
  }
};

const showCodeFilename = () => {
  var els = document.getElementsByClassName("highlight");
  for (var i = 0; i < els.length; i++) {
    if (els[i].title.length) {
      var newNode = document.createElement("div");
      newNode.innerHTML = `<span class="code-sample-filename">${els[i].title}</span>`;
      els[i].parentNode.insertBefore(newNode, els[i]);
    }
  }
};

// Text of a code block, without line numbers. Returns null if the code can't be identified.
const codeBlockText = (block) => {
  // Table-style line numbers render two <pre> elements; the code is in the last cell.
  const pre = block.querySelector(".lntd:last-child pre") || block.querySelector("pre");
  if (!pre) return null;

  const clone = pre.cloneNode(true);
  clone.querySelectorAll(".ln, .lnt").forEach((el) => el.remove());
  const text = clone.textContent.replace(/\n$/, "");
  return text.length ? text : null;
};

const svgIcon = (body) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const copyIcon = svgIcon(
  '<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>'
);
const copiedIcon = svgIcon('<polyline points="20 6 9 17 4 12"/>');
const failedIcon = svgIcon(
  '<circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>'
);

// Copy buttons for code blocks in page content
const addCopyButtons = () => {
  if (!navigator.clipboard?.writeText) return;

  // Raw HTML code blocks (e.g. in generated component descriptions) lack Hugo's .highlight wrapper
  document.querySelectorAll("#page-content pre.chroma").forEach((pre) => {
    if (pre.closest(".highlight")) return;
    const wrapper = document.createElement("div");
    wrapper.className = "highlight";
    pre.replaceWith(wrapper);
    wrapper.append(pre);
  });

  document.querySelectorAll("#page-content .highlight").forEach((block) => {
    if (codeBlockText(block) === null) return;

    const button = document.createElement("button");
    button.type = "button";
    button.className = "code-copy-button";
    button.setAttribute("aria-label", "Copy code to clipboard");
    button.title = "Copy";
    button.innerHTML = copyIcon;

    const status = document.createElement("span");
    status.className = "sr-only";
    status.setAttribute("aria-live", "polite");

    let resetTimer;
    let copying = false;
    button.addEventListener("click", async () => {
      // Guard with a flag rather than `disabled`, which can drop keyboard focus
      if (copying) return;
      copying = true;
      clearTimeout(resetTimer);
      try {
        await navigator.clipboard.writeText(codeBlockText(block));
        button.innerHTML = copiedIcon;
        button.title = status.textContent = "Copied!";
      } catch {
        button.innerHTML = failedIcon;
        button.title = status.textContent = "Copy failed. Select the code to copy it manually.";
      } finally {
        copying = false;
      }
      resetTimer = setTimeout(() => {
        button.innerHTML = copyIcon;
        button.title = "Copy";
        status.textContent = "";
      }, 2000);
    });

    block.classList.add("has-copy-button");
    block.append(button, status);
  });
};

document.addEventListener("DOMContentLoaded", () => {
  // search.start();

  tableOfContents();
  showCodeFilename();
  addCopyButtons();
});
