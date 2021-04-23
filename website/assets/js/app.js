import '@ryangjchandler/spruce';
import 'alpinejs';
import 'tocbot/dist/tocbot';

// Table of contents for documentation pages

tocbot.init({
  tocSelector: '#docs-toc',
  contentSelector: '#docs-content',
  headingSelector: 'h1, h2, h3, h4',
  ignoreSelector: 'no-toc',
  scrollSmoothDuration: 400
});

/* Global state management */

// Dark mode state
const manageState = () => {
  window.Spruce.store('dark', {
    enabled: true,
    toggle() {
      this.enabled = !this.enabled;
    }
  }, true);
}

const sayHello = () => {
  console.log('Welcome to the Vector website and documentation!');
}

const main = () => {
  sayHello();
  manageState();
}

document.addEventListener("DOMContentLoaded", main());
