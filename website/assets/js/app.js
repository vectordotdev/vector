{{ $latest := index site.Params.vector_versions 0 }}
import '@ryangjchandler/spruce';
import 'alpinejs';
import 'tocbot/dist/tocbot';

const sayHello = () => {
  console.log('Welcome to the Vector website and documentation!');
}

// Table of contents for documentation pages
const tableOfContents = () => {
  tocbot.init({
    tocSelector: '#toc',
    contentSelector: '#page-content',
    headingSelector: 'h1, h2, h3, h4',
    ignoreSelector: 'no-toc',
    scrollSmoothDuration: 400
  });
}

/* Global state management */

const manageState = () => {
  // Persist global state in localStorage
  const useLocalStorage = true;

  // Detect the user's dark mode preference and set that to the default
  const darkModeDefault = window.matchMedia('(prefers-color-scheme: dark)').matches;

  window.Spruce.store('global', {
    // Dark mode state
    dark: darkModeDefault,
    // Whether the top banner is showing (user can dismiss)
    banner: true,
    // The Vector version selected (for the download and releases pages)
    version: '{{ $latest }}',
    // Release version
    release: 'stable',
    // Set a new version
    setVersion(v) {
      this.version = v;
    },
    // Switch dark mode on and off
    toggleDarkMode() {
      this.dark = !this.dark;
    },
    // Toggle between stable and nightly
    toggleRelease() {
      if (this.release === 'stable') {
        this.release = 'nightly';
      } else if (this.release === 'nightly') {
        this.release = 'stable';
      }
    },
    isNightly() {
      return this.release === 'nightly';
    },
    isCurrent(version) {
      return this.version === version;
    },
    // Set release directly
    setRelease(release) {
      this.release = release;
    },
    // Switch the banner on and off
    toggleBanner() {
      this.banner = !this.banner;
    }
  }, useLocalStorage);
}

const showCodeFilename = () => {
  const classes = "font-semibold font-mono tracking-wider text-gray-50 dark:text-gray-200 bg-dark dark:bg-gray-700 py-1.5 px-2 rounded text-xs";
  var els = document.getElementsByClassName("highlight");
  for (var i = 0; i < els.length; i++) {
    if (els[i].title.length) {
      var newNode = document.createElement("div");
      newNode.innerHTML = `<span class="${classes}">${els[i].title}</span>`;
      newNode.classList.add("code-title");
      els[i].parentNode.insertBefore(newNode, els[i]);
    }
  }
}

const main = () => {
  sayHello();
  manageState();
  tableOfContents();
  showCodeFilename();
}

document.addEventListener("DOMContentLoaded", main());
