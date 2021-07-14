import 'tocbot/dist/tocbot';

// Table of contents for documentation pages
const tableOfContents = () => {
  if (document.getElementById('toc')) {
    tocbot.init({
      tocSelector: '#toc',
      contentSelector: '#page-content',
      headingSelector: 'h2,h3,h4,h5',
      scrollSmoothDuration: 400
    });
  }
}

const showCodeFilename = () => {
  var els = document.getElementsByClassName("highlight");
  for (var i = 0; i < els.length; i++) {
    if (els[i].title.length) {
      var newNode = document.createElement("div");
      newNode.innerHTML = `<span class="code-sample-filename">${els[i].title}</span>`;
      els[i].parentNode.insertBefore(newNode, els[i]);
    }
  }
}

document.addEventListener('DOMContentLoaded', () => {
  // search.start();

  tableOfContents();
  showCodeFilename();
});
