module.exports = {
  title: 'Vector',
  tagline: 'A High-Performance, Logs, Metrics, & Events Router',
  url: 'https://vector.dev',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'timberio',
  projectName: 'vector',
  themeConfig: {
    navbar: {
      logo: {
        alt: 'Vector',
        src: 'img/logo-light.svg',
        darkSrc: 'img/logo-dark.svg'
      },
      links: [
        {to: 'use_cases', label: 'Use Cases', position: 'right'},
        {to: 'docs/components', label: 'Integrations', position: 'right'},
        {href: '/docs', label: 'Docs', position: 'right'},
        {to: 'blog', label: 'Blog', position: 'right'},
        {to: 'community', label: 'Community', position: 'right'},
        {
          href: 'https://github.com/timberio/vector',
          label: "GitHub",
          position: 'right',
        },
      ],
    },
    prismTheme: require('prism-react-renderer/themes/github'),
    darkPrismTheme: require('prism-react-renderer/themes/dracula')
  },
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          editUrl: 'https://github.com/timberio/vector/edit/master/website/docs/',
          sidebarPath: require.resolve('./sidebars.js'),
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
  scripts: [],
  stylesheets: [
    'https://fonts.googleapis.com/css?family=Ubuntu|Roboto|Source+Code+Pro',
    'https://at-ui.github.io/feather-font/css/iconfont.css'
  ],
};
