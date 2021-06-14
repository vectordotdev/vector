import 'tocbot/dist/tocbot';

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

const main = () => {
  tableOfContents();
}

main();
