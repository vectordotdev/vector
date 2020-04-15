export interface GuideContent {
  guides: Guide[];
  guideTags: GuideTags;
  guideCategories: GuideCategory[];
}

export interface PluginOptions {
  path: string;
  routeBasePath: string;
  include: string[];
  guideListComponent: string;
  guideComponent: string;
  guideTagListComponent: string;
  guideTagComponent: string;
  guideCategoryComponent: string;
  remarkPlugins: string[];
  rehypePlugins: string[];
  truncateMarker: RegExp;
}

export interface GuideCategory {
  name: string;
  title: string;
  description: string | null;
  permalink: string;
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

export interface MetaData {
  categories: GuideCategory[];
  coverLabel: string;
  description: string;
  nextItem?: Paginator;
  permalink: string;
  prevItem?: Paginator;
  readingTime: string;
  seriesPosition: number;
  sort: number;
  source: string;
  tags: (Tag | string)[];
  title: string;
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
