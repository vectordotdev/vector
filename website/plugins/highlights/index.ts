import _ from 'lodash';
import path from 'path';
import {normalizeUrl, docuHash, aliasedSitePath} from '@docusaurus/utils';

import {
  PluginOptions,
  Highlight,
  HighlightContent,
  HighlightItemsToMetadata,
  HighlightTags,
  TagsModule,
} from './types';
import {
  LoadContext,
  PluginContentLoadedActions,
  ConfigureWebpackUtils,
  Plugin,
} from '@docusaurus/types';
import {Configuration, Loader} from 'webpack';
import {generateHighlights} from './highlightUtils';

const DEFAULT_OPTIONS: PluginOptions = {
  path: 'highlights', // Path to data on filesystem, relative to site dir.
  routeBasePath: 'highlights', // URL Route.
  include: ['*.md', '*.mdx'], // Extensions to include.
  highlightComponent: '@theme/HighlightPage',
  highlightListComponent: '@theme/HighlightListPage',
  highlightTagComponent: '@theme/HighlightTagPage',
  highlightTagListComponent: '@theme/HighlightTagListPage',
  remarkPlugins: [],
  rehypePlugins: [],
  truncateMarker: /<!--\s*(truncate)\s*-->/, // Regex.
};

export default function pluginContentHighlight(
  context: LoadContext,
  opts: Partial<PluginOptions>,
): Plugin<HighlightContent | null> {
  const options: PluginOptions = {...DEFAULT_OPTIONS, ...opts};
  const {siteDir, generatedFilesDir} = context;
  const contentPath = path.resolve(siteDir, options.path);
  const dataDir = path.join(
    generatedFilesDir,
    'highlights',
  );
  let highlights: Highlight[] = [];

  return {
    name: 'highlights',

    getPathsToWatch() {
      const {include = []} = options;
      const highlightsGlobPattern = include.map(pattern => `${contentPath}/${pattern}`);
      return [...highlightsGlobPattern];
    },

    async loadContent() {
      const {routeBasePath} = options;
      const {siteConfig: {baseUrl = ''}} = context;
      const basePageUrl = normalizeUrl([baseUrl, routeBasePath]);

      //
      // Highlights
      //

      highlights = await generateHighlights(contentPath, context, options);

      // Colocate next and prev metadata.
      highlights.forEach((highlight, index) => {
        const prevItem = index > 0 ? highlights[index - 1] : null;
        if (prevItem) {
          highlight.metadata.prevItem = {
            title: prevItem.metadata.title,
            permalink: prevItem.metadata.permalink,
          };
        }

        const nextItem = index < highlights.length - 1 ? highlights[index + 1] : null;
        if (nextItem) {
          highlight.metadata.nextItem = {
            title: nextItem.metadata.title,
            permalink: nextItem.metadata.permalink,
          };
        }
      });

      //
      // Highlight tags
      //

      const highlightTags: HighlightTags = {};
      const tagsPath = normalizeUrl([basePageUrl, 'tags']);
      highlights.forEach(highlight => {
        const {tags} = highlight.metadata;
        if (!tags || tags.length === 0) {
          // TODO: Extract tags out into a separate plugin.
          // eslint-disable-next-line no-param-reassign
          highlight.metadata.tags = [];
          return;
        }

        // eslint-disable-next-line no-param-reassign
        highlight.metadata.tags = tags.map(tag => {
          if (typeof tag === 'string') {
            const normalizedTag = _.kebabCase(tag);
            const permalink = normalizeUrl([tagsPath, normalizedTag]);
            if (!highlightTags[normalizedTag]) {
              highlightTags[normalizedTag] = {
                // Will only use the name of the first occurrence of the tag.
                name: tag.toLowerCase(),
                items: [],
                permalink,
              };
            }

            highlightTags[normalizedTag].items.push(highlight.id);

            return {
              label: tag,
              permalink,
            };
          } else {
            return tag;
          }
        });
      });

      //
      // Return
      //

      return {
        highlights,
        highlightTags,
      };
    },

    async contentLoaded({
      content: highlightContents,
      actions,
    }: {
      content: HighlightContent;
      actions: PluginContentLoadedActions;
    }) {
      if (!highlightContents) {
        return;
      }

      //
      // Prepare
      //

      const {
        highlightComponent,
        highlightListComponent,
        highlightTagComponent,
        highlightTagListComponent,
      } = options;

      const aliasedSource = (source: string) =>
        `~highlight/${path.relative(dataDir, source)}`;
      const {addRoute, createData} = actions;
      const {highlights, highlightTags} = highlightContents;
      const {routeBasePath} = options;
      const {siteConfig: {baseUrl = ''}} = context;
      const basePageUrl = normalizeUrl([baseUrl, routeBasePath]);

      const highlightItemsToMetadata: HighlightItemsToMetadata = {};
      highlights.map(highlight => {
        highlightItemsToMetadata[highlight.id] = highlight.metadata;
      });

      //
      // Highlights page
      //

      addRoute({
        path: basePageUrl,
        component: highlightListComponent,
        exact: true,
        modules: {
          items: highlights.map(highlight => {
            const metadata = highlight.metadata;
            // To tell routes.js this is an import and not a nested object to recurse.
            return {
              content: {
                __import: true,
                path: metadata.source,
                query: {
                  truncated: true,
                },
              },
            };
          }),
        },
      });

      //
      // Highlight tags
      //

      const tagsPath = normalizeUrl([basePageUrl, 'tags']);
      const tagsModule: TagsModule = {};

      await Promise.all(
        Object.keys(highlightTags).map(async tag => {
          const {name, items, permalink} = highlightTags[tag];

          tagsModule[tag] = {
            allTagsPath: tagsPath,
            slug: tag,
            name,
            count: items.length,
            permalink,
          };

          const tagsMetadataPath = await createData(
            `${docuHash(permalink)}.json`,
            JSON.stringify(tagsModule[tag], null, 2),
          );

          addRoute({
            path: permalink,
            component: highlightTagComponent,
            exact: true,
            modules: {
              items: items.map(highlightID => {
                const metadata = highlightItemsToMetadata[highlightID];
                return {
                  content: {
                    __import: true,
                    path: metadata.source,
                    query: {
                      truncated: true,
                    },
                  },
                };
              }),
              metadata: aliasedSource(tagsMetadataPath),
            },
          });
        }),
      );

      const tagsListPath = await createData(
        `${docuHash(`${tagsPath}-tags`)}.json`,
        JSON.stringify(tagsModule, null, 2),
      );

      addRoute({
        path: tagsPath,
        component: highlightTagListComponent,
        exact: true,
        modules: {
          tags: aliasedSource(tagsListPath),
        },
      });

      //
      // Highlight pages
      //

      await Promise.all(
        highlights.map(async highlight => {
          const {metadata} = highlight;
          await createData(
            // Note that this created data path must be in sync with
            // metadataPath provided to mdx-loader.
            `${docuHash(metadata.source)}.json`,
            JSON.stringify(metadata, null, 2),
          );

          addRoute({
            path: metadata.permalink,
            component: highlightComponent,
            exact: true,
            modules: {
              content: metadata.source,
            },
          });
        }),
      );
    },

    configureWebpack(
      _config: Configuration,
      isServer: boolean,
      {getBabelLoader, getCacheLoader}: ConfigureWebpackUtils,
    ) {
      const {rehypePlugins, remarkPlugins, truncateMarker} = options;
      return {
        resolve: {
          alias: {
            '~highlight': dataDir,
          },
        },
        module: {
          rules: [
            {
              test: /(\.mdx?)$/,
              include: [contentPath],
              use: [
                getCacheLoader(isServer),
                getBabelLoader(isServer),
                {
                  loader: '@docusaurus/mdx-loader',
                  options: {
                    remarkPlugins,
                    rehypePlugins,
                    // Note that metadataPath must be the same/in-sync as
                    // the path from createData for each MDX.
                    metadataPath: (mdxPath: string) => {
                      const aliasedSource = aliasedSitePath(mdxPath, siteDir);
                      return path.join(
                        dataDir,
                        `${docuHash(aliasedSource)}.json`,
                      );
                    },
                  },
                },
                {
                  loader: path.resolve(__dirname, './markdownLoader.js'),
                  options: {
                    siteDir,
                    contentPath,
                    truncateMarker,
                    highlights,
                  },
                },
              ].filter(Boolean) as Loader[],
            },
          ],
        },
      };
    },

    injectHtmlTags() {
      return {}
    },
  };
}
