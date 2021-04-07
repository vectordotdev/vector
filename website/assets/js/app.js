import 'alpinejs';
import 'tocbot/dist/tocbot';

// Table of contents for documentation pages
tocbot.init({
  tocSelector: '#docs-toc',
  contentSelector: '#docs-content',
  headingSelector: 'h1, h2, h3',
  ignoreSelector: 'no-toc',
  scrollSmoothDuration: 400
});

console.log('Welcome to the Vector website and documentation!');
