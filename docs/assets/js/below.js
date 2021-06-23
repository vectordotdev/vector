{{ $search := site.Params.search }}
{{ $algoliaAppId := $search.algolia_app_id }}
{{ $algoliaApiKey := $search.algola_public_api_key }}
{{ $algoliaIndex := cond hugo.IsProduction $search.algolia_index_prod $search.algolia_index_staging }}

import algoliasearch from 'algoliasearch/lite';
import instantsearch from 'instantsearch.js';
import 'tocbot/dist/tocbot';

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

// Algolia search
const searchClient = algoliasearch('{{ $algoliaAppId }}', '{{ $algoliaApiKey }}');

const search = instantsearch({
  indexName: '{{ $algoliaIndex }}',
  searchClient
});

const searchBox = instantsearch.widgets.searchBox({
  container: '#algolia-search-box'
});

const searchHits = instantsearch.widgets.hits({
  container: '#algolia-search-hits'
})

search.addWidgets([
  searchBox,
  searchHits
]);

document.addEventListener('DOMContentLoaded', () => {
  search.start();

  tableOfContents();
});
