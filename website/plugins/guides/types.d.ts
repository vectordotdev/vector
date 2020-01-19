/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
export interface GuideContent {
    guides: Guide[];
    guideListPaginated: GuidePaginated[];
    guideTags: GuideTags;
    guideTagsListPath: string | null;
}
export interface DateLink {
    date: Date;
    link: string;
}
export declare type FeedType = 'rss' | 'atom' | 'all';
export interface PluginOptions {
    path: string;
    routeBasePath: string;
    include: string[];
    guidesPerPage: number;
    guideListComponent: string;
    guideComponent: string;
    guideTagsListComponent: string;
    guideTagsGuidesComponent: string;
    remarkPlugins: string[];
    rehypePlugins: string[];
    truncateMarker: RegExp;
    feedOptions?: {
        type: FeedType;
        title?: string;
        description?: string;
        copyright: string;
        language?: string;
    };
}
export interface GuideTags {
    [key: string]: GuideTag;
}
export interface GuideTag {
    name: string;
    items: string[];
    permalink: string;
}
export interface Guide {
    id: string;
    metadata: MetaData;
}
export interface GuidePaginatedMetadata {
    permalink: string;
    page: number;
    guidesPerPage: number;
    totalPages: number;
    totalCount: number;
    previousPage: string | null;
    nextPage: string | null;
}
export interface GuidePaginated {
    metadata: GuidePaginatedMetadata;
    items: string[];
}
export interface MetaData {
    permalink: string;
    source: string;
    description: string;
    date: Date;
    tags: (Tag | string)[];
    title: string;
    prevItem?: Paginator;
    nextItem?: Paginator;
    truncated: boolean;
}
export interface Paginator {
    title: string;
    permalink: string;
}
export interface Tag {
    label: string;
    permalink: string;
}
export interface GuideItemsToMetadata {
    [key: string]: MetaData;
}
export interface TagsModule {
    [key: string]: TagModule;
}
export interface TagModule {
    allTagsPath: string;
    slug: string;
    name: string;
    count: number;
    permalink: string;
}
//# sourceMappingURL=types.d.ts.map