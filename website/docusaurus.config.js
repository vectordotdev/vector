module.exports = {
  title: 'Vector',
  tagline: 'A High-Performance, Logs, Metrics, & Events Router',
  url: 'https://vector.dev',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'timberio',
  projectName: 'vector',
  customFields: {
    metadata: require('./metadata'),
  },
  themeConfig: {
    navbar: {
      logo: {
        alt: 'Vector',
        src: 'img/logo-light.svg',
        darkSrc: 'img/logo-dark.svg'
      },
      links: [
        {to: 'components', label: 'Components', position: 'right'},
        {to: 'docs', label: 'Docs', position: 'right'},
        {to: 'blog', label: 'Blog', position: 'right'},
        {to: 'download', label: 'Download', position: 'right'},
        {
          href: 'https://github.com/timberio/vector',
          label: "GitHub",
          position: 'right',
        },
      ],
    },
    prism: {
      theme: require('prism-react-renderer/themes/github'),
      darkTheme: require('prism-react-renderer/themes/dracula'),
    },
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
  scripts: [
    'https://cdnjs.cloudflare.com/ajax/libs/svg.js/2.7.1/svg.min.js',
    'https://cdnjs.cloudflare.com/ajax/libs/jquery/3.4.1/jquery.min.js',
  ],
  stylesheets: [
    'https://fonts.googleapis.com/css?family=Ubuntu|Roboto|Source+Code+Pro',
    'https://at-ui.github.io/feather-font/css/iconfont.css',
  ],
};
