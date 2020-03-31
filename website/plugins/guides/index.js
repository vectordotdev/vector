"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const lodash_1 = __importDefault(require("lodash"));
const path_1 = __importDefault(require("path"));
const utils_1 = require("@docusaurus/utils");
const guideUtils_1 = require("./guideUtils");
const DEFAULT_OPTIONS = {
    path: 'guides',
    routeBasePath: 'guides',
    include: ['**/*.md', '**/*.mdx'],
    guideListComponent: '@theme/GuideListPage',
    guideComponent: '@theme/GuidePage',
    guideTagListComponent: '@theme/GuideTagListPage',
    guideTagComponent: '@theme/GuideTagPage',
    guideCategoryComponent: '@theme/GuideCategoryPage',
    remarkPlugins: [],
    rehypePlugins: [],
    truncateMarker: /<!--\s*(truncate)\s*-->/,
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
            const { routeBasePath } = options;
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
            // Example: `/guides`
            const totalCount = guides.length;
            const { siteConfig: { baseUrl = '' }, } = context;
            const basePageUrl = utils_1.normalizeUrl([baseUrl, routeBasePath]);
            const guideListPaginated = [];
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
            // Guide categories
            let guideCategories = lodash_1.default.flatMap(guides, (guide => guide.metadata.categories));
            guideCategories = lodash_1.default.uniqBy(guideCategories, ((guideCategory) => guideCategory.permalink));
            return {
                guides,
                guideListPaginated,
                guideTags,
                guideTagsListPath,
                guideCategories,
            };
        },
        async contentLoaded({ content: guideContents, actions, }) {
            if (!guideContents) {
                return;
            }
            const { guideListComponent, guideComponent, guideTagListComponent, guideTagComponent, guideCategoryComponent, } = options;
            const aliasedSource = (source) => `~guide/${path_1.default.relative(dataDir, source)}`;
            const { addRoute, createData } = actions;
            const { guides, guideListPaginated, guideTags, guideTagsListPath, guideCategories, } = guideContents;
            const guideItemsToMetadata = {};
            guides.map(guide => {
                guideItemsToMetadata[guide.id] = guide.metadata;
            });
            // Guides list
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
            // Tags
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
            }));
            // Only create /tags page if there are tags.
            if (Object.keys(guideTags).length > 0) {
                const tagsListPath = await createData(`${utils_1.docuHash(`${guideTagsListPath}-tags`)}.json`, JSON.stringify(tagsModule, null, 2));
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
                let guidePermalinks = lodash_1.default.uniq(guides.map(guide => guide.metadata.permalink));
                await Promise.all(guideCategories.
                    filter(category => !guidePermalinks.includes(category.permalink)).
                    map(async (category) => {
                    const permalink = category.permalink;
                    const metadata = { category: category };
                    const categoryMetadataPath = await createData(`${utils_1.docuHash(permalink)}.json`, JSON.stringify(metadata, null, 2));
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
                }));
            }
            // Guide pages
            await Promise.all(guides.map(async (guide) => {
                const { metadata } = guide;
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
            }));
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
        injectHtmlTags() {
            return {};
        },
    };
}
exports.default = pluginContentGuide;
