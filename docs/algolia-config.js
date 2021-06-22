module.exports = {
  alternativesAsExact: [
    "ignorePlurals",
    "singleWordSynonym"
  ],
  attributeForDistinct: null,
  attributesForFaceting: null,
  attributesToHighlight: null,
  attributesToRetrieve: null,
  attributesToSnippet: [
    "title:10",
    "content:10"
  ],
  customRanking: [
    "desc(level)",
    "desc(ranking)"
  ],
  exactOnSingleWordQuery: "attribute",
  highlightPreTag: "<em>",
  highlightPostTag: "</em>",
  hitsPerPage: 20,
  maxValuesPerFacet: 100,
  minWordSizefor1Typo: 4,
  minWordSizefor2Typos: 8,
  numericAttributesToIndex: null,
  optionalWords: null,
  paginationLimitedTo: 1000,
  queryType: "prefixLast",
  ranking: [
    "typo",
    "geo",
    "words",
    "filters",
    "proximity",
    "attribute",
    "exact",
    "custom"
  ],
  removeWordsIfNoResults: "none",
  rules: [],
  searchableAttributes: [
    "title",
    "content",
    "unordered(tags)"
  ],
  separatorsToIndex: "",
  snippetEllipsisText: "...",
  synonyms: [],
  unretrievableAttributes: null
};
