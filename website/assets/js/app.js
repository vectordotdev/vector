{{ $latest := index site.Data.docs.versions 0 }}
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
    headingSelector: 'h1, h2, h3, h4, h5',
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
    // A "backup" version for use in release toggling
    versionBackup: '{{ $latest }}',
    // Release version
    release: 'stable',

    // Set release directly
    setRelease(release) {
      this.release = release;
    },
    // Set a new version
    setVersion(v) {
      this.version = v;

      if (v === 'nightly') {
        this.setRelease('nightly');
      }

      if (v != 'nightly') {
        this.setRelease('stable');
        this.versionBackup = v;
      }
    },
    notLatest() {
      return this.version != '{{ $latest }}';
    },
    setToLatest() {
      this.setVersion('{{ $latest }}');
    },
    // Switch dark mode on and off
    toggleDarkMode() {
      this.dark = !this.dark;
    },
    // Toggle between stable and nightly
    toggleRelease() {
      if (this.release === 'stable') {
        this.release = 'nightly';
        this.setVersion('nightly');
      } else if (this.release === 'nightly') {
        this.release = 'stable';
        this.setVersion(this.versionBackup);
      }
    },
    // Switch the banner on and off
    toggleBanner() {
      this.banner = !this.banner;
    },
    // Boolean helpers
    isNightly() {
      return this.release === 'nightly';
    },
    isStable() {
      return this.release === 'stable';
    },
    isCurrent(version) {
      return this.version === version;
    },
  }, useLocalStorage);

  window.Spruce.store('ui', {
    // Management UI data
    {{ range site.Data.docs.administration.ui.management.families }}
    {{ .name }}_interface: '{{ (index .interfaces 0).title }}',
    {{ end }}


    platform: '{{ index site.Data.docs.administration.ui.management.family_names 0 }}',
    interface: '{{ site.Data.ui.defaults.interface }}',
    dockerVersion: '{{ site.Data.ui.default.dockerVersion }}',
    dockerDistro: '{{ site.Data.ui.default.dockerDistro }}',
  }, useLocalStorage);
}

const showCodeFilename = () => {
  const classes = "code-title font-semibold font-mono tracking-wide text-gray-50 dark:text-gray-200 bg-dark dark:bg-black py-1.5 px-2 rounded text-sm";
  var els = document.getElementsByClassName("highlight");
  for (var i = 0; i < els.length; i++) {
    if (els[i].title.length) {
      var newNode = document.createElement("div");
      newNode.innerHTML = `<span class="${classes}">${els[i].title}</span>`;
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

main();
