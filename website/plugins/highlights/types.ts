export interface Highlight {
  id: string;
  metadata: MetaData;
}

export interface HighlightContent {
  highlights: Highlight[];
  highlightTags: HighlightTags;
}

export interface HighlightItemsToMetadata {
  [key: string]: MetaData;
}

export interface HighlightTag {
  name: string;
  items: string[];
  permalink: string;
}

export interface HighlightTags {
  [key: string]: HighlightTag;
}

export interface MetaData {
  date: Date;
  description: string;
  nextItem?: Paginator;
  permalink: string;
  prevItem?: Paginator;
  readingTime: string;
  source: string;
  tags: (Tag | string)[];
  title: string;
  truncated: boolean;
}

export interface Paginator {
  title: string;
  permalink: string;
}

export interface PluginOptions {
  path: string;
  routeBasePath: string;
  include: string[];
  highlightComponent: string;
  highlightListComponent: string;
  highlightTagListComponent: string;
  highlightTagComponent: string;
  remarkPlugins: string[];
  rehypePlugins: string[];
  truncateMarker: RegExp;
}

export interface Tag {
  label: string;
  permalink: string;
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
