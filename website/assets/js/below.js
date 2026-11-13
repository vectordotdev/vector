import tocbot from "tocbot";

// Table of contents for documentation pages
const tableOfContents = () => {
  if (document.getElementById("toc")) {
    const content = document.getElementById("page-content");
    tocbot.init({
      tocSelector: "#toc",
      contentSelector: "#page-content",
      headingSelector: content?.dataset.tocHeadings || "h2,h3,h4,h5",
      includeTitleTags: Boolean(content?.dataset.tocHeadings),
      headingObjectCallback: (heading, element) => {
        // Notice cards sit below their section; headings inside their bodies are excluded.
        if (element.matches(".changelog-card")) {
          return {
            ...heading,
            nodeName: "H3",
            headingLevel: 3,
            textContent: element.querySelector("[data-toc-title]").textContent.trim()
          };
        }
        return heading;
      },
      scrollSmoothDuration: 400,
      headingsOffset: 80,
      scrollSmoothOffset: -80,
      hasInnerContainers: true
    });
  }
};

document.addEventListener("deprecations-sorted", tableOfContents);

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

// Animate the open/close of collapsible content so its height transitions in sync with the
// chevron rotation, instead of the native instant show/hide. Duration scales with the height
// change so tall content doesn't feel like it snaps open or closed.
const DISCLOSURE_PX_PER_MS = 3;
const DISCLOSURE_MIN_DURATION = 200;
const DISCLOSURE_MAX_DURATION = 1500;

const disclosureTransitionDuration = (height) => {
  const duration = height / DISCLOSURE_PX_PER_MS;
  return Math.min(DISCLOSURE_MAX_DURATION, Math.max(DISCLOSURE_MIN_DURATION, duration));
};

const disclosureAnimations = new WeakMap();

// Animate `content` from its current height (which may be mid-animation) to its full height or
// to zero. `setVisible` shows the content before expanding and hides it after collapsing.
const toggleDisclosure = (content, chevron, expand, setVisible) => {
  // Content in a closed <details> keeps its layout height, so check visibility too.
  const startHeight = content.checkVisibility() ? content.getBoundingClientRect().height : 0;
  disclosureAnimations.get(content)?.cancel();
  if (expand) setVisible(true);
  const endHeight = expand ? content.scrollHeight : 0;
  const duration = disclosureTransitionDuration(Math.abs(endHeight - startHeight));

  if (chevron) {
    chevron.style.transitionDuration = `${duration}ms`;
    chevron.classList.toggle("is-expanded", expand);
  }

  const animation = content.animate([{ height: `${startHeight}px` }, { height: `${endHeight}px` }], {
    duration,
    easing: "ease",
    fill: "forwards"
  });
  disclosureAnimations.set(content, animation);
  animation.onfinish = () => {
    disclosureAnimations.delete(content);
    if (!expand) setVisible(false);
    animation.cancel();
  };
};

// `collapsible` shortcode
const animateDisclosureDetails = () => {
  document.addEventListener("click", (e) => {
    const summary = e.target.closest(".disclosure-details > summary");
    if (!summary) return;
    if (e.target.closest("a")) return;

    e.preventDefault();

    const details = summary.parentElement;
    const content = details.querySelector(".disclosure-content");
    const chevron = details.querySelector(".disclosure-chevron");
    // `open` stays set while collapsing, so the chevron tracks the intended state instead.
    const expand = !chevron.classList.contains("is-expanded");
    toggleDisclosure(content, chevron, expand, (visible) => {
      details.open = visible;
    });
  });
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
const externalLinkIcon = svgIcon(
  '<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>'
);
const chevronIcon =
  '<svg class="disclosure-chevron" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd" /></svg>';

// Copy the full snippet even when its content is collapsed.
const addCopyButton = (block, controls) => {
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

  controls.append(button, status);
};

const addSourceLink = (url, controls) => {
  const link = document.createElement("a");
  link.className = "code-source-link";
  link.href = url;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  link.title = "View on GitHub";
  link.setAttribute("aria-label", "View on GitHub");
  link.innerHTML = externalLinkIcon;
  controls.append(link);
};

// Longer snippets, and snippets with a filename, get a collapsible header.
const collapsibleCodeLineCount = 20;

const addCodeControls = () => {
  // Plain-text and raw HTML code blocks lack Hugo's .highlight wrapper.
  document.querySelectorAll("#page-content pre").forEach((pre) => {
    if (pre.closest(".highlight")) return;
    const wrapper = document.createElement("div");
    wrapper.className = "highlight";
    pre.replaceWith(wrapper);
    wrapper.append(pre);
  });

  document.querySelectorAll("#page-content .highlight").forEach((block, index) => {
    const text = codeBlockText(block);
    if (text === null) return;

    const lineCount = text.split("\n").length;
    const collapsedByDefault = block.parentElement.dataset.codeCollapsed === "true";
    const filename = block.parentElement.dataset.codeFilename;
    const collapsible = collapsedByDefault || Boolean(filename) || lineCount > collapsibleCodeLineCount;
    const canCopy = Boolean(navigator.clipboard?.writeText);
    if (!collapsible && !canCopy) return;

    const content = block.firstElementChild;
    content.classList.add("code-block-content");
    block.classList.add("has-code-controls");

    const controls = document.createElement("div");
    controls.className = "code-block-controls";

    if (collapsible) {
      block.classList.add("is-collapsible");

      // Animate a wrapper: the content scrolls horizontally, so clipping it would add a vertical scrollbar.
      const wrapper = document.createElement("div");
      wrapper.className = "disclosure-content";
      wrapper.id = `code-block-content-${index}`;
      wrapper.hidden = collapsedByDefault;
      content.replaceWith(wrapper);
      wrapper.append(content);

      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "code-toggle-button";
      toggle.innerHTML = chevronIcon;
      toggle.setAttribute("aria-expanded", String(!wrapper.hidden));
      toggle.setAttribute("aria-controls", wrapper.id);
      toggle.title = wrapper.hidden ? "Expand" : "Collapse";

      const chevron = toggle.firstElementChild;
      chevron.classList.toggle("is-expanded", !wrapper.hidden);

      const label = document.createElement("span");
      label.className = "code-line-count";
      label.textContent = filename ? `${filename} · ${lineCount} lines` : `${lineCount} lines`;
      toggle.append(label);

      toggle.addEventListener("click", () => {
        const expand = toggle.getAttribute("aria-expanded") === "false";
        toggle.setAttribute("aria-expanded", String(expand));
        toggle.title = expand ? "Collapse" : "Expand";
        toggleDisclosure(wrapper, chevron, expand, (visible) => {
          wrapper.hidden = !visible;
        });
      });

      controls.append(toggle);
      block.prepend(controls);
    } else {
      block.append(controls);
    }

    const url = block.parentElement.dataset.codeUrl;
    if (url) addSourceLink(url, controls);
    if (canCopy) addCopyButton(block, controls);
  });
};

document.addEventListener("DOMContentLoaded", () => {
  // search.start();

  tableOfContents();
  showCodeFilename();
  animateDisclosureDetails();
  addCodeControls();

  // Code controls change the layout after Firefox may have followed the URL fragment.
  document.querySelector(":target")?.scrollIntoView();
});
