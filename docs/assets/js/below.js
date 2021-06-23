{{ $search := site.Params.search }}
{{ $algoliaAppId := $search.algolia_app_id }}
{{ $algoliaApiKey := $search.algola_public_api_key }}
{{ $algoliaIndex := cond hugo.IsProduction $search.algolia_index_prod $search.algolia_index_staging }}

import algoliasearch from 'algoliasearch/lite';
import instantsearch from 'instantsearch.js';
import { searchBox } from 'instantsearch.js/es/widgets';
import { connectHits } from 'instantsearch.js/es/connectors';

import 'tocbot/dist/tocbot';

// Algolia search
const searchClient = algoliasearch('{{ $algoliaAppId }}', '{{ $algoliaApiKey }}');

const search = instantsearch({
  indexName: '{{ $algoliaIndex }}',
  searchClient
});

const searchInput = searchBox({
  container: '#algolia-search-box'
});

const renderHits = (renderOptions, _isFirstRender) => {
  const { hits, widgetParams } = renderOptions;

  widgetParams.container.innerHTML = `
    <ul class="flex flex-col divide-y dark:divide-gray-700">
      ${hits
        .map(
          item =>
            `<li class="text-dark dark:text-gray-200 py-2 hover:text-secondary dark:hover:text-primary">
              <a href="${item.url}">
              ${instantsearch.highlight({ attribute: 'title', hit: item })}
              </a>
            </li>`
        )
        .join('')}
    </ul>
  `;
};

const customHits = connectHits(
  renderHits
);

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

document.addEventListener('DOMContentLoaded', () => {
  search.start();

  tableOfContents();
});
