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

// Animate the open/close of `embed` shortcode <details> elements so the
// content height transitions in sync with the chevron rotation, instead of
// the native instant show/hide. Duration scales with content height so tall
// embeds don't feel like they snap open/closed.
const EMBED_PX_PER_MS = 3;
const EMBED_MIN_DURATION = 200;
const EMBED_MAX_DURATION = 1500;

const embedTransitionDuration = (height) => {
  const duration = height / EMBED_PX_PER_MS;
  return Math.min(EMBED_MAX_DURATION, Math.max(EMBED_MIN_DURATION, duration));
};

const animateEmbedDetails = () => {
  document.addEventListener("click", (e) => {
    const summary = e.target.closest(".embed-details > summary");
    if (!summary) return;
    if (e.target.closest("a")) return;

    e.preventDefault();

    const details = summary.parentElement;
    const content = details.querySelector(".embed-content");
    const chevron = details.querySelector(".embed-chevron");

    if (details.open) {
      const startHeight = content.scrollHeight;
      const duration = embedTransitionDuration(startHeight);
      content.style.transitionDuration = `${duration}ms`;
      if (chevron) chevron.style.transitionDuration = `${duration}ms`;
      content.style.height = `${startHeight}px`;
      content.getBoundingClientRect();

      requestAnimationFrame(() => {
        content.style.height = "0px";
      });

      content.addEventListener(
        "transitionend",
        function handler() {
          details.open = false;
          content.style.height = "";
          content.style.transitionDuration = "";
          if (chevron) chevron.style.transitionDuration = "";
          content.removeEventListener("transitionend", handler);
        },
        { once: true }
      );
    } else {
      details.open = true;
      const endHeight = content.scrollHeight;
      const duration = embedTransitionDuration(endHeight);
      content.style.transitionDuration = `${duration}ms`;
      if (chevron) chevron.style.transitionDuration = `${duration}ms`;
      content.style.height = "0px";
      content.getBoundingClientRect();

      requestAnimationFrame(() => {
        content.style.height = `${endHeight}px`;
      });

      content.addEventListener(
        "transitionend",
        function handler() {
          content.style.height = "";
          content.style.transitionDuration = "";
          if (chevron) chevron.style.transitionDuration = "";
          content.removeEventListener("transitionend", handler);
        },
        { once: true }
      );
    }
  });
};

document.addEventListener("DOMContentLoaded", () => {
  // search.start();

  tableOfContents();
  showCodeFilename();
  animateEmbedDetails();
});
