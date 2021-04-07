import 'alpinejs';
import 'tocbot/dist/tocbot';

tocbot.init({
  tocSelector: '#docs-toc',
  contentSelector: '#docs-content',
  headingSelector: 'h1, h2, h3',
  ignoreSelector: 'no-toc'
});

console.log('Welcome to the Vector website and documentation!');
