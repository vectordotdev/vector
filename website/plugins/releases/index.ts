import _ from 'lodash';
import path from 'path';
import {normalizeUrl, docuHash, aliasedSitePath} from '@docusaurus/utils';

import {
  PluginOptions,
  Release,
  ReleaseContent,
} from './types';
import {
  LoadContext,
  PluginContentLoadedActions,
  ConfigureWebpackUtils,
  Plugin,
} from '@docusaurus/types';
import {Configuration, Loader} from 'webpack';
import {generateReleases} from './releaseUtils';

const DEFAULT_OPTIONS: PluginOptions = {
  path: 'releases', // Path to data on filesystem, relative to site dir.
  routeBasePath: 'releases', // URL Route.
  include: ['*.md', '*.mdx'], // Extensions to include.
  releaseComponent: '@theme/ReleasePage',
  releaseDownloadComponent: '@theme/ReleaseDownloadPage',
  remarkPlugins: [],
  rehypePlugins: [],
  truncateMarker: /<!--\s*(truncate)\s*-->/, // Regex.
};

export default function pluginContentRelease(
  context: LoadContext,
  opts: Partial<PluginOptions>,
): Plugin<ReleaseContent | null> {
  const options: PluginOptions = {...DEFAULT_OPTIONS, ...opts};
  const {siteDir, generatedFilesDir} = context;
  const contentPath = path.resolve(siteDir, options.path);
  const dataDir = path.join(
    generatedFilesDir,
    'releases',
  );
  let releases: Release[] = [];

  return {
    name: 'releases',

    getPathsToWatch() {
      const {include = []} = options;
      const releasesGlobPattern = include.map(pattern => `${contentPath}/${pattern}`);
      return [...releasesGlobPattern];
    },

    async loadContent() {
      //
      // Releases
      //

      releases = await generateReleases(contentPath, context, options);

      // Colocate next and prev metadata.
      releases.forEach((release, index) => {
        const prevItem = index > 0 ? releases[index - 1] : null;
        if (prevItem) {
          release.metadata.prevItem = {
            title: prevItem.metadata.title,
            permalink: prevItem.metadata.permalink,
          };
        }

        const nextItem = index < releases.length - 1 ? releases[index + 1] : null;
        if (nextItem) {
          release.metadata.nextItem = {
            title: nextItem.metadata.title,
            permalink: nextItem.metadata.permalink,
          };
        }
      });

      //
      // Return
      //

      return {
        releases,
      };
    },

    async contentLoaded({
      content: releaseContents,
      actions,
    }: {
      content: ReleaseContent;
      actions: PluginContentLoadedActions;
    }) {
      if (!releaseContents) {
        return;
      }

      //
      // Prepare
      //

      const {
        releaseComponent,
        releaseDownloadComponent,
      } = options;

      const {addRoute, createData} = actions;
      const {releases} = releaseContents;

      //
      // Release pages
      //

      await Promise.all(
        releases.map(async release => {
          const {metadata} = release;
          await createData(
            // Note that this created data path must be in sync with
            // metadataPath provided to mdx-loader.
            `${docuHash(metadata.source)}.json`,
            JSON.stringify(metadata, null, 2),
          );

          addRoute({
            path: metadata.permalink,
            component: releaseComponent,
            exact: true,
            modules: {
              content: metadata.source,
            },
          });

          let downloadPath = normalizeUrl([metadata.permalink, 'download']);

          addRoute({
            path: downloadPath,
            component: releaseDownloadComponent,
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
            '~release': dataDir,
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
                    releases,
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
