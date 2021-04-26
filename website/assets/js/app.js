import '@ryangjchandler/spruce';
import 'alpinejs';
import 'tocbot/dist/tocbot';

// Table of contents for documentation pages
const tableOfContents = () => {
  tocbot.init({
    tocSelector: '#docs-toc',
    contentSelector: '#docs-content',
    headingSelector: 'h1, h2, h3, h4',
    ignoreSelector: 'no-toc',
    scrollSmoothDuration: 400
  });

  tocbot.init({
    tocSelector: '#docs-toc-slideover',
    contentSelector: '#docs-content',
    headingSelector: 'h1, h2, h3, h4',
    ignoreSelector: 'no-toc',
    scrollSmoothDuration: 400
  });
}

/* Global state management */

// Dark mode state
const manageState = () => {
  const useLocalStorage = true;

  // Detect the user's dark mode preference and set that to the default
  const darkModeDefault = window.matchMedia('(prefers-color-scheme: dark)').matches;

  window.Spruce.store('global', {
    dark: darkModeDefault,
    banner: true,
    toggleDarkMode() {
      this.dark = !this.dark;
    },
    toggleBanner() {
      this.banner = !this.banner;
    }
  }, useLocalStorage);
}

const sayHello = () => {
  console.log('Welcome to the Vector website and documentation!');
}

const main = () => {
  sayHello();
  manageState();
  tableOfContents();
  showCodeFilename();
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

document.addEventListener("DOMContentLoaded", main());
