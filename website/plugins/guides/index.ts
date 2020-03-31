import _ from 'lodash';
import path from 'path';
import {normalizeUrl, docuHash, aliasedSitePath} from '@docusaurus/utils';

import {
  PluginOptions,
  GuideItemsToMetadata,
  GuideTags,
  GuideContent,
  TagsModule,
  GuidePaginated,
  Guide,
} from './types';
import {
  LoadContext,
  PluginContentLoadedActions,
  ConfigureWebpackUtils,
  Plugin,
} from '@docusaurus/types';
import {Configuration, Loader} from 'webpack';
import {generateGuides} from './guideUtils';

const DEFAULT_OPTIONS: PluginOptions = {
  path: 'guides', // Path to data on filesystem, relative to site dir.
  routeBasePath: 'guides', // URL Route.
  include: ['**/*.md', '**/*.mdx'], // Extensions to include.
  guideListComponent: '@theme/GuideListPage',
  guideComponent: '@theme/GuidePage',
  guideTagListComponent: '@theme/GuideTagListPage',
  guideTagComponent: '@theme/GuideTagPage',
  guideCategoryComponent: '@theme/GuideCategoryPage',
  remarkPlugins: [],
  rehypePlugins: [],
  truncateMarker: /<!--\s*(truncate)\s*-->/, // Regex.
};

export default function pluginContentGuide(
  context: LoadContext,
  opts: Partial<PluginOptions>,
): Plugin<GuideContent | null> {
  const options: PluginOptions = {...DEFAULT_OPTIONS, ...opts};
  const {siteDir, generatedFilesDir} = context;
  const contentPath = path.resolve(siteDir, options.path);
  const dataDir = path.join(
    generatedFilesDir,
    'guides',
  );
  let guides: Guide[] = [];

  return {
    name: 'guides',

    getPathsToWatch() {
      const {include = []} = options;
      const globPattern = include.map(pattern => `${contentPath}/${pattern}`);
      return [...globPattern];
    },

    // Fetches guide contents and returns metadata for the necessary routes.
    async loadContent() {
      const {routeBasePath} = options;

      guides = await generateGuides(contentPath, context, options);
      if (!guides.length) {
        return null;
      }

      // Colocate next and prev metadata.
      guides.forEach((guide, index) => {
        const prevItem = index > 0 ? guides[index - 1] : null;
        if (prevItem) {
          guide.metadata.prevItem = {
            title: prevItem.metadata.title,
            permalink: prevItem.metadata.permalink,
          };
        }

        const nextItem =
          index < guides.length - 1 ? guides[index + 1] : null;
        if (nextItem) {
          guide.metadata.nextItem = {
            title: nextItem.metadata.title,
            permalink: nextItem.metadata.permalink,
          };
        }
      });

      // Guide pagination routes.
      // Example: `/guides`
      const totalCount = guides.length;
      const {
        siteConfig: {baseUrl = ''},
      } = context;
      const basePageUrl = normalizeUrl([baseUrl, routeBasePath]);

      const guideListPaginated: GuidePaginated[] = [];

      guideListPaginated.push({
        metadata: {
          permalink: basePageUrl,
          page: 1,
          guidesPerPage: 1,
          totalPages: 1,
          totalCount,
          previousPage: basePageUrl,
          nextPage: basePageUrl,
        },
        items: guides.filter(guide => guide.metadata.categories.length <= 2).map(item => item.id),
      });

      // Guide tags
      const guideTags: GuideTags = {};
      const tagsPath = normalizeUrl([basePageUrl, 'tags']);
      guides.forEach(guide => {
        const {tags} = guide.metadata;
        if (!tags || tags.length === 0) {
          // TODO: Extract tags out into a separate plugin.
          // eslint-disable-next-line no-param-reassign
          guide.metadata.tags = [];
          return;
        }

        // eslint-disable-next-line no-param-reassign
        guide.metadata.tags = tags.map(tag => {
          if (typeof tag === 'string') {
            const normalizedTag = _.kebabCase(tag);
            const permalink = normalizeUrl([tagsPath, normalizedTag]);
            if (!guideTags[normalizedTag]) {
              guideTags[normalizedTag] = {
                // Will only use the name of the first occurrence of the tag.
                name: tag.toLowerCase(),
                items: [],
                permalink,
              };
            }

            guideTags[normalizedTag].items.push(guide.id);

            return {
              label: tag,
              permalink,
            };
          } else {
            return tag;
          }
        });
      });

      const guideTagsListPath =
        Object.keys(guideTags).length > 0 ? tagsPath : null;


      // Guide categories
      let guideCategories = _.flatMap(guides, (guide => guide.metadata.categories));
      guideCategories = _.uniqBy(guideCategories, ((guideCategory) => guideCategory.permalink));

      return {
        guides,
        guideListPaginated,
        guideTags,
        guideTagsListPath,
        guideCategories,
      };
    },

    async contentLoaded({
      content: guideContents,
      actions,
    }: {
      content: GuideContent;
      actions: PluginContentLoadedActions;
    }) {
      if (!guideContents) {
        return;
      }

      const {
        guideListComponent,
        guideComponent,
        guideTagListComponent,
        guideTagComponent,
        guideCategoryComponent,
      } = options;

      const aliasedSource = (source: string) =>
        `~guide/${path.relative(dataDir, source)}`;
      const {addRoute, createData} = actions;
      const {
        guides,
        guideListPaginated,
        guideTags,
        guideTagsListPath,
        guideCategories,
      } = guideContents;

      const guideItemsToMetadata: GuideItemsToMetadata = {};
      guides.map(guide => {
        guideItemsToMetadata[guide.id] = guide.metadata;
      });

      // Guides list
      await Promise.all(
        guideListPaginated.map(async listPage => {
          const {metadata, items} = listPage;
          const {permalink} = metadata;
          const pageMetadataPath = await createData(
            `${docuHash(permalink)}.json`,
            JSON.stringify(metadata, null, 2),
          );

          addRoute({
            path: permalink,
            component: guideListComponent,
            exact: true,
            modules: {
              items: items.map(guideID => {
                const metadata = guideItemsToMetadata[guideID];
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
              metadata: aliasedSource(pageMetadataPath),
            },
          });
        }),
      );

      // Tags
      if (guideTagsListPath === null) {
        return;
      }

      const tagsModule: TagsModule = {};

      await Promise.all(
        Object.keys(guideTags).map(async tag => {
          const {name, items, permalink} = guideTags[tag];

          tagsModule[tag] = {
            allTagsPath: guideTagsListPath,
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
            component: guideTagComponent,
            exact: true,
            modules: {
              items: items.map(guideID => {
                const metadata = guideItemsToMetadata[guideID];
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

      // Only create /tags page if there are tags.
      if (Object.keys(guideTags).length > 0) {
        const tagsListPath = await createData(
          `${docuHash(`${guideTagsListPath}-tags`)}.json`,
          JSON.stringify(tagsModule, null, 2),
        );

        addRoute({
          path: guideTagsListPath,
          component: guideTagListComponent,
          exact: true,
          modules: {
            tags: aliasedSource(tagsListPath),
          },
        });
      }

      // Guide categories
      if (guideCategories.length > 0) {
        let guidePermalinks = _.uniq(guides.map(guide => guide.metadata.permalink));

        await Promise.all(
          guideCategories.
            filter(category => !guidePermalinks.includes(category.permalink)).
            map(async category => {
              const permalink = category.permalink;
              const metadata = {category: category};
              const categoryMetadataPath = await createData(
                `${docuHash(permalink)}.json`,
                JSON.stringify(metadata, null, 2),
              );

              addRoute({
                path: permalink,
                component: guideCategoryComponent,
                exact: true,
                modules: {
                  items: guides.
                    filter(guide => guide.metadata.categories.map(category => category.permalink).includes(category.permalink)).
                    map(guide => {
                      const metadata = guideItemsToMetadata[guide.id];
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
                  metadata: aliasedSource(categoryMetadataPath),
                },
              });
            })
        );
      }

      // Guide pages
      await Promise.all(
        guides.map(async guide => {
          const {metadata} = guide;
          await createData(
            // Note that this created data path must be in sync with
            // metadataPath provided to mdx-loader.
            `${docuHash(metadata.source)}.json`,
            JSON.stringify(metadata, null, 2),
          );

          addRoute({
            path: metadata.permalink,
            component: guideComponent,
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
            '~guide': dataDir,
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
                    guides,
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
