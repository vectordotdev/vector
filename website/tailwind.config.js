const defaultTheme = require('tailwindcss/defaultTheme')

module.exports = {
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        'primary': '#28d9f2',
        'secondary': '#f44af5',
        'twitter-blue': '#1DA1F2',
        'discord-purple': '#7289DA',
        'rss-orange': '#f26522'
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
