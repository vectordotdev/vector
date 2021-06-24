const postcssImport = require('postcss-import');
const tailwindCss = require('tailwindcss');
const autoprefixer = require('autoprefixer')({
  browsers: ['last 2 versions']
});

// These are classes for things that are applied by JS, and thus missed by Hugo.
// See assets/js/*.js for places where this happens.
const safeClasses = [
  'bg-dark',
  'dark:bg-black',
  'dark:hover:text-primary',
  'dark:text-gray-200',
  'focus:bg-white',
  'focus:border-none',
  'focus:outline-none',
  'focus:ring-none',
  'focus:text-gray-900',
  'font-mono',
  'font-semibold',
  'hover:text-secondary',
  'px-2',
  'py-1.5',
  'py-2',
  'rounded',
  'text-dark',
  'text-gray-50',
  'text-sm',
  'tracking-wide',
];

const purgecss = require('@fullhuman/postcss-purgecss')({
  content: ['./hugo_stats.json'],
  safelist: safeClasses,
  defaultExtractor: (content) => {
      let els = JSON.parse(content).htmlElements;
      return els.tags.concat(els.classes, els.ids);
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
