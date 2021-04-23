const defaultTheme = require('tailwindcss/defaultTheme')

module.exports = {
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        'dark': '#191927',
        'primary': '#28d9f2',
        'primary-dark': '#00a9bc',
        'secondary': '#f44af5',
        'purple': '#98f',
        'twitter-blue': '#1DA1F2',
        'discord-purple': '#7289DA',
        'rss-orange': '#f26522'
      },
      fontFamily: {
        sans: ['Segoe UI', ...defaultTheme.fontFamily.sans]
      },
      gridTemplateColumns: {
        '16': 'repeat(16, minmax(0, 1fr))',
      },
      typography: (theme) => ({
        dark: {
          css: {
            color: theme('colors.gray.100'),
            'p, h1, h2, h3, h4, h5, h6': {
              color: theme('colors.gray.100')
            },
            'a, a code, p code': {
              color: theme('colors.primary'),
              'text-decoration': 'none',
              '&:hover, &:active': {
                color: 'secondary',
              },
            },
            strong: {
              color: theme('colors.gray.100'),
            },
          }
        }
      }),
    },
  },
  variants: {
    extend: {
      typography: ['dark']
    },
  },
  plugins: [
    require('@tailwindcss/forms'),
    require('@tailwindcss/typography'),
  ],
}
