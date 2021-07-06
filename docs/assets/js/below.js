{{ $search := site.Params.search }}
{{ $algoliaAppId := $search.algolia_app_id }}
{{ $algoliaApiKey := $search.algola_public_api_key }}
{{ $algoliaIndex := cond hugo.IsProduction $search.algolia_index_prod $search.algolia_index_staging }}

import algoliasearch from 'algoliasearch/lite';
import instantsearch from 'instantsearch.js';
import { connectHits, connectSearchBox } from 'instantsearch.js/es/connectors';

import 'tocbot/dist/tocbot';

// Algolia search
const searchClient = algoliasearch('{{ $algoliaAppId }}', '{{ $algoliaApiKey }}');

const search = instantsearch({
  indexName: '{{ $algoliaIndex }}',
  searchClient
});

const renderSearchBox = (renderOptions, isFirstRender) => {
  const { query, refine, widgetParams } = renderOptions;

  const container = widgetParams.container;

  // Make sure you add any Tailwind classes you apply here to the safeClasses list in postcss.config.js
  if (isFirstRender) {
    container.innerHTML = `
    <input x-model="query" x-ref="q" id="algolia-search-input" autocomplete="on" aria-label="Search" aria-autocomplete="list" aria-owns="algolia-search-results" spellcheck="false" dir="auto" name="search" type="search" class="search-input" placeholder="Search">
    `;

    container.querySelector('#algolia-search-input').addEventListener('input', event => {
      refine(event.target.value);
    });
  }

  container.querySelector('#algolia-search-input').value = query;
}

const renderHits = (renderOptions, _isFirstRender) => {
  const { hits, widgetParams } = renderOptions;

  // Make sure you add any Tailwind classes you apply here to the safeClasses list in postcss.config.js
  widgetParams.container.innerHTML = `
    <ul class="search-results-list">
      ${hits
        .map(
          item =>
            `<li class="search-result">
               <a href="${item.url}">
               ${instantsearch.highlight({ attribute: 'title', hit: item, highlightedTagName: "strong" })}
               </a>
             </li>`
        )
        .join('')}
    </ul>
  `;
};

const customHits = connectHits(renderHits);

const customSearchBox = connectSearchBox(renderSearchBox);

const searchInput = customSearchBox({
  container: document.querySelector('#algolia-search-box')
});

const searchResults = customHits({
  container: document.querySelector('#algolia-search-results')
});

search.addWidgets([
  searchInput,
  searchResults
]);

// Table of contents for documentation pages
const tableOfContents = () => {
  if (document.getElementById('toc')) {
    tocbot.init({
      tocSelector: '#toc',
      contentSelector: '#page-content',
      headingSelector: 'h1, h2, h3, h4, h5',
      ignoreSelector: 'no-toc',
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
  search.start();

  tableOfContents();
  showCodeFilename();
});
