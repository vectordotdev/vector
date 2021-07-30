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
    /^DocSearch/,
    // Search widgets (TODO: improve this by consolidating these into higher classes)
    "block",
    "border-gray-200",
    "border-gray-300",
    "border-r",
    "border-t",
    "font-medium",
    "h-2",
    "h-3",
    "h-full",
    "inline",
    "leading-relaxed",
    "mb-1",
    "ml-1",
    "mr-1",
    "p-2",
    "p-4",
    "pl-2",
    "py-4",
    "text-gray-600",
    "text-gray-800",
    "text-md",
    "text-sm",
    "w-2",
    "w-3",
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
