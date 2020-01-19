"use strict";
/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_extra_1 = __importDefault(require("fs-extra"));
const lodash_1 = __importDefault(require("lodash"));
const path_1 = __importDefault(require("path"));
const utils_1 = require("@docusaurus/utils");
const guideUtils_1 = require("./guideUtils");
const DEFAULT_OPTIONS = {
    path: 'guides',
    routeBasePath: 'guides',
    include: ['**/*.md', '**/*.mdx'],
    guidesPerPage: 10,
    guideListComponent: '@theme/GuideListPage',
    guideComponent: '@theme/GuidePage',
    guideTagsListComponent: '@theme/GuideTagsListPage',
    guideTagsGuidesComponent: '@theme/GuideTagsGuidesPage',
    remarkPlugins: [],
    rehypePlugins: [],
    truncateMarker: /<!--\s*(truncate)\s*-->/,
};
function assertFeedTypes(val) {
    if (typeof val !== 'string' && !['rss', 'atom', 'all'].includes(val)) {
        throw new Error(`Invalid feedOptions type: ${val}. It must be either 'rss', 'atom', or 'all'`);
    }
}
const getFeedTypes = (type) => {
    assertFeedTypes(type);
    let feedTypes = [];
    if (type === 'all') {
        feedTypes = ['rss', 'atom'];
    }
    else {
        feedTypes.push(type);
    }
    return feedTypes;
};
function pluginContentGuide(context, opts) {
    const options = Object.assign(Object.assign({}, DEFAULT_OPTIONS), opts);
    const { siteDir, generatedFilesDir } = context;
    const contentPath = path_1.default.resolve(siteDir, options.path);
    const dataDir = path_1.default.join(generatedFilesDir, 'guides');
    let guides = [];
    return {
        name: 'guides',
        getPathsToWatch() {
            const { include = [] } = options;
            const globPattern = include.map(pattern => `${contentPath}/${pattern}`);
            return [...globPattern];
        },
        // Fetches guide contents and returns metadata for the necessary routes.
        async loadContent() {
            const { guidesPerPage, routeBasePath } = options;
            guides = await guideUtils_1.generateGuides(contentPath, context, options);
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
                const nextItem = index < guides.length - 1 ? guides[index + 1] : null;
                if (nextItem) {
                    guide.metadata.nextItem = {
                        title: nextItem.metadata.title,
                        permalink: nextItem.metadata.permalink,
                    };
                }
            });
            // Guide pagination routes.
            // Example: `/guide`, `/guide/page/1`, `/guide/page/2`
            const totalCount = guides.length;
            const numberOfPages = Math.ceil(totalCount / guidesPerPage);
            const { siteConfig: { baseUrl = '' }, } = context;
            const basePageUrl = utils_1.normalizeUrl([baseUrl, routeBasePath]);
            const guideListPaginated = [];
            function guidePaginationPermalink(page) {
                return page > 0
                    ? utils_1.normalizeUrl([basePageUrl, `page/${page + 1}`])
                    : basePageUrl;
            }
            for (let page = 0; page < numberOfPages; page += 1) {
                guideListPaginated.push({
                    metadata: {
                        permalink: guidePaginationPermalink(page),
                        page: page + 1,
                        guidesPerPage,
                        totalPages: numberOfPages,
                        totalCount,
                        previousPage: page !== 0 ? guidePaginationPermalink(page - 1) : null,
                        nextPage: page < numberOfPages - 1
                            ? guidePaginationPermalink(page + 1)
                            : null,
                    },
                    items: guides
                        .slice(page * guidesPerPage, (page + 1) * guidesPerPage)
                        .map(item => item.id),
                });
            }
            const guideTags = {};
            const tagsPath = utils_1.normalizeUrl([basePageUrl, 'tags']);
            guides.forEach(guide => {
                const { tags } = guide.metadata;
                if (!tags || tags.length === 0) {
                    // TODO: Extract tags out into a separate plugin.
                    // eslint-disable-next-line no-param-reassign
                    guide.metadata.tags = [];
                    return;
                }
                // eslint-disable-next-line no-param-reassign
                guide.metadata.tags = tags.map(tag => {
                    if (typeof tag === 'string') {
                        const normalizedTag = lodash_1.default.kebabCase(tag);
                        const permalink = utils_1.normalizeUrl([tagsPath, normalizedTag]);
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
                    }
                    else {
                        return tag;
                    }
                });
            });
            const guideTagsListPath = Object.keys(guideTags).length > 0 ? tagsPath : null;
            return {
                guides,
                guideListPaginated,
                guideTags,
                guideTagsListPath,
            };
        },
        async contentLoaded({ content: guideContents, actions, }) {
            if (!guideContents) {
                return;
            }
            const { guideListComponent, guideComponent, guideTagsListComponent, guideTagsGuidesComponent, } = options;
            const aliasedSource = (source) => `~guide/${path_1.default.relative(dataDir, source)}`;
            const { addRoute, createData } = actions;
            const { guides, guideListPaginated, guideTags, guideTagsListPath, } = guideContents;
            const guideItemsToMetadata = {};
            // Create routes for guide entries.
            await Promise.all(guides.map(async (guide) => {
                const { id, metadata } = guide;
                await createData(
                // Note that this created data path must be in sync with
                // metadataPath provided to mdx-loader.
                `${utils_1.docuHash(metadata.source)}.json`, JSON.stringify(metadata, null, 2));
                addRoute({
                    path: metadata.permalink,
                    component: guideComponent,
                    exact: true,
                    modules: {
                        content: metadata.source,
                    },
                });
                guideItemsToMetadata[id] = metadata;
            }));
            // Create routes for guide's paginated list entries.
            await Promise.all(guideListPaginated.map(async (listPage) => {
                const { metadata, items } = listPage;
                const { permalink } = metadata;
                const pageMetadataPath = await createData(`${utils_1.docuHash(permalink)}.json`, JSON.stringify(metadata, null, 2));
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
            }));
            // Tags.
            if (guideTagsListPath === null) {
                return;
            }
            const tagsModule = {};
            await Promise.all(Object.keys(guideTags).map(async (tag) => {
                const { name, items, permalink } = guideTags[tag];
                tagsModule[tag] = {
                    allTagsPath: guideTagsListPath,
                    slug: tag,
                    name,
                    count: items.length,
                    permalink,
                };
                const tagsMetadataPath = await createData(`${utils_1.docuHash(permalink)}.json`, JSON.stringify(tagsModule[tag], null, 2));
                addRoute({
                    path: permalink,
                    component: guideTagsGuidesComponent,
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
            }));
            // Only create /tags page if there are tags.
            if (Object.keys(guideTags).length > 0) {
                const tagsListPath = await createData(`${utils_1.docuHash(`${guideTagsListPath}-tags`)}.json`, JSON.stringify(tagsModule, null, 2));
                addRoute({
                    path: guideTagsListPath,
                    component: guideTagsListComponent,
                    exact: true,
                    modules: {
                        tags: aliasedSource(tagsListPath),
                    },
                });
            }
        },
        configureWebpack(_config, isServer, { getBabelLoader, getCacheLoader }) {
            const { rehypePlugins, remarkPlugins, truncateMarker } = options;
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
                                        metadataPath: (mdxPath) => {
                                            const aliasedSource = utils_1.aliasedSitePath(mdxPath, siteDir);
                                            return path_1.default.join(dataDir, `${utils_1.docuHash(aliasedSource)}.json`);
                                        },
                                    },
                                },
                                {
                                    loader: path_1.default.resolve(__dirname, './markdownLoader.js'),
                                    options: {
                                        siteDir,
                                        contentPath,
                                        truncateMarker,
                                        guides,
                                    },
                                },
                            ].filter(Boolean),
                        },
                    ],
                },
            };
        },
        async postBuild({ outDir }) {
            var _a;
            if (!options.feedOptions) {
                return;
            }
            const feed = await guideUtils_1.generateGuideFeed(context, options);
            if (!feed) {
                return;
            }
            const feedTypes = getFeedTypes((_a = options.feedOptions) === null || _a === void 0 ? void 0 : _a.type);
            await Promise.all(feedTypes.map(feedType => {
                const feedPath = path_1.default.join(outDir, options.routeBasePath, `${feedType}.xml`);
                const feedContent = feedType === 'rss' ? feed.rss2() : feed.atom1();
                return fs_extra_1.default.writeFile(feedPath, feedContent, err => {
                    if (err) {
                        throw new Error(`Generating ${feedType} feed failed: ${err}`);
                    }
                });
            }));
        },
        injectHtmlTags() {
            var _a;
            if (!options.feedOptions) {
                return {};
            }
            const feedTypes = getFeedTypes((_a = options.feedOptions) === null || _a === void 0 ? void 0 : _a.type);
            const { siteConfig: { title }, baseUrl, } = context;
            const feedsConfig = {
                rss: {
                    type: 'application/rss+xml',
                    path: 'guide/rss.xml',
                    title: `${title} Guide RSS Feed`,
                },
                atom: {
                    type: 'application/atom+xml',
                    path: 'guide/atom.xml',
                    title: `${title} Guide Atom Feed`,
                },
            };
            const headTags = [];
            feedTypes.map(feedType => {
                const feedConfig = feedsConfig[feedType] || {};
                if (!feedsConfig) {
                    return;
                }
                const { type, path, title } = feedConfig;
                headTags.push({
                    tagName: 'link',
                    attributes: {
                        rel: 'alternate',
                        type,
                        href: utils_1.normalizeUrl([baseUrl, path]),
                        title,
                    },
                });
            });
            return {
                headTags,
            };
        },
    };
}
exports.default = pluginContentGuide;
