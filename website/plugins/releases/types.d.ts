export interface PluginOptions {
    path: string;
    routeBasePath: string;
    include: string[];
    releaseComponent: string;
    releaseDownloadComponent: string;
    remarkPlugins: string[];
    rehypePlugins: string[];
    truncateMarker: RegExp;
}
export interface Paginator {
    title: string;
    permalink: string;
}
export interface ReleaseContent {
    releases: Release[];
}
export interface Tag {
    label: string;
    permalink: string;
}
export interface Release {
    id: string;
    metadata: Metdata;
}
export interface Metdata {
    date: Date;
    description: string;
    nextItem?: Paginator;
    permalink: string;
    prevItem?: Paginator;
    source: string;
    title: string;
    truncated: boolean;
    version: string;
}
//# sourceMappingURL=types.d.ts.map