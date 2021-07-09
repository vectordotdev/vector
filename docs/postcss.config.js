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
    "type",
    // All Algolia-specific classes
    /^ais-/,
    /^aa-/,
    // Search widgets (TODO: improve this by consolidating these into higher classes)
    "pl-2",
    "text-gray-800",
    "text-md",
    "mb-1",
    "font-medium",
    "leading-relaxed",
    "text-gray-600",
    "text-sm",
    "border-gray-200",
    "border-r",
    "p-2",
    "p-4"
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
