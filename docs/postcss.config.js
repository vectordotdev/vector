const postcssImport = require('postcss-import');
const tailwindCss = require('tailwindcss');
const autoprefixer = require('autoprefixer')({
  browsers: ['last 2 versions']
});

// These are classes for things that are applied by JS, and thus missed by Hugo.
// See assets/js/*.js for places where this happens.
const safeClasses = {
  standard: [
    "search-input",
    "search-results-list",
    "search-result",
    "code-sample-filename",
    /^ais-/, // All Algolia-specific classes
    "type"
  ]
};

const purgecss = require('@fullhuman/postcss-purgecss')({
  content: ['./hugo_stats.json'],
  safelist: safeClasses,
  defaultExtractor: (content) => {
    const broadMatches = content.match(/[^<>"'`\s]*[^<>"'`\s:]/g) || [];
    const innerMatches = content.match(/[^<>"'`\s.()]*[^<>"'`\s.():]/g) || [];
    return broadMatches.concat(innerMatches);
  }
})

module.exports = {
  plugins: [
    postcssImport,
    tailwindCss,
    autoprefixer,
    ...(process.env.HUGO_ENVIRONMENT === 'production' ? [purgecss] : [])
  ]
}
