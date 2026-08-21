{{ $latest := index site.Data.docs.versions 0 }}
{{ $defaultPlatformTab := index site.Home.Params.platform.tabs 0 }}
{{ $siteGeneration := site.Params.site_generation }}
import Alpine from 'alpinejs';
import persist from '@alpinejs/persist';

Alpine.plugin(persist);

const sayHello = () => {
  console.log('Welcome to the Vector website and documentation!');
}

const clearLocalStorageOnNewGeneration = () => {
  const currentGeneration = {{ $siteGeneration }};
  const storedGeneration = localStorage.getItem('generation');

  if ((storedGeneration != null) && (storedGeneration < currentGeneration)) {
    Object.keys(localStorage)
      .filter((item) => item.startsWith('__alpine:global:'))
      .forEach((item) => localStorage.removeItem(item));
  }

  localStorage.setItem('generation', currentGeneration);
}

/* Global state management */
const manageState = () => {
  // Detect the user's dark mode preference and set that to the default
  const darkModeDefault = window.matchMedia('(prefers-color-scheme: dark)').matches;
  const persistValue = (key, defaultValue) =>
    Alpine.$persist(defaultValue)
      .using(localStorage)
      .as(`__alpine:global:${key}`);

  Alpine.store('global', {
    // Dark mode state
    dark: persistValue('dark', darkModeDefault),
    // Whether the top banner is showing (user can dismiss)
    banner: persistValue('banner', true),
    // The Vector version selected (for the download and releases pages)
    version: persistValue('version', '{{ $latest }}'),
    // A "backup" version for use in release toggling
    versionBackup: persistValue('versionBackup', '{{ $latest }}'),
    // Release version
    release: persistValue('release', 'stable'),
    // Home page platform tab
    platformTab: persistValue('platformTab', '{{ $defaultPlatformTab }}'),
    // Config format
    format: persistValue('format', 'yaml'),

    // Helper functions
    setFormat(f) {
      this.format = f;
    },

    isFormat(f) {
      return this.format === f;
    },

    isDark() {
      return this.dark;
    },

    isLight() {
      return !this.dark;
    },

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
    isVersion(v) {
      return this.version === v;
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
    // Switch the banner on and off
    toggleBanner() {
      this.banner = !this.banner;
    },
    // Boolean helpers
    isNightly() {
      return this.release === 'nightly';
    },
  });

}

const main = () => {
  sayHello();
  clearLocalStorageOnNewGeneration();
  manageState();
  Alpine.start();
}

main();
