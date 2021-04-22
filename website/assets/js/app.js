import '@ryangjchandler/spruce';
import 'alpinejs';
import 'tocbot/dist/tocbot';

// Table of contents for documentation pages

/*
tocbot.init({
  tocSelector: '#docs-toc',
  contentSelector: '#docs-content',
  headingSelector: 'h1, h2, h3, h4',
  ignoreSelector: 'no-toc',
  scrollSmoothDuration: 400
});
*/

/* Global state management */

// Dark mode state
window.Spruce.store('dark', {
  enabled: null,
  toggle() {
    this.enabled = !this.enabled;
  }
}, true);

console.log('Welcome to the Vector website and documentation!');
