const defaultTheme = require('tailwindcss/defaultTheme')

module.exports = {
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        'primary': '#28d9f2',
        'twitter-blue': '#1DA1F2'
      },
      fontFamily: {
        sans: ['Segoe UI', ...defaultTheme.fontFamily.sans],
      }
    },
  },
  variants: {
    extend: {},
  },
  plugins: [
    require('@tailwindcss/forms')
  ],
}
