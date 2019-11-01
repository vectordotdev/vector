module.exports = {
  title: 'My Site',
  tagline: 'The tagline of my site',
  url: 'https://your-docusaurus-test-site.com',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'facebook', // Usually your GitHub org/user name.
  projectName: 'docusaurus', // Usually your repo name.
  themeConfig: {
    navbar: {
      logo: {
        alt: 'Vector',
        src: 'img/logo.svg',
      },
      links: [
        {to: 'use_cases', label: 'Use Cases', position: 'right'},
        {to: 'Integrations', label: 'Integrations', position: 'right'},
        {to: 'docs/README', label: 'Docs', position: 'right'},
        {to: 'blog', label: 'Blog', position: 'right'},
        {to: 'community', label: 'Community', position: 'right'},
        {
          href: 'https://github.com/timberio/vector',
          label: "GitHub",
          icon: "test",
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Docs',
              to: 'docs/doc1',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'Discord',
              href: 'https://discordapp.com/invite/docusaurus',
            },
          ],
        },
        {
          title: 'Social',
          items: [
            {
              label: 'Blog',
              to: 'blog',
            },
          ],
        },
      ],
      logo: {
        alt: 'Facebook Open Source Logo',
        src: 'https://docusaurus.io/img/oss_logo.png',
      },
      copyright: `Copyright © ${new Date().getFullYear()} Facebook, Inc. Built with Docusaurus.`,
    },
    prismTheme: require('prism-react-renderer/themes/github'),
  },
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          editUrl: 'https://github.com/facebook/docusaurus/edit/master/website/docs/',
          path: '/Users/benjohnson/Code/timber/vector/docs',
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
    'https://at.alicdn.com/t/font_o5hd5vvqpoqiwwmi.css'
  ]
};
