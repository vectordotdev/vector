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

  const focus = 'focus:outline-none focus:bg-white focus:text-gray-900 focus:ring-none focus:border-none';

  if (isFirstRender) {
    container.innerHTML = `
    <input x-model="query" x-ref="q" id="algolia-search-input" name="search" type="search" class="dark:bg-gray-700 dark:text-gray-400 bg-gray-200 text-gray-800 block w-full pl-10 pr-3 border border-transparent rounded-md leading-5 placeholder-gray-400 sm:text-sm ${focus}" placeholder="Search">
    `;

    container.querySelector('#algolia-search-input').addEventListener('input', event => {
      refine(event.target.value);
    });
  }

  container.querySelector('#algolia-search-input').value = query;
}

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

document.addEventListener('DOMContentLoaded', () => {
  search.start();

  tableOfContents();
});
