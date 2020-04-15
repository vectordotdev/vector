import { PluginOptions, Highlight } from './types';
import { LoadContext } from '@docusaurus/types';
export declare function truncate(fileString: string, truncateMarker: RegExp): string;
export declare function generateHighlights(highlightDir: string, { siteConfig, siteDir }: LoadContext, options: PluginOptions): Promise<Highlight[]>;
export declare function linkify(fileContent: string, siteDir: string, highlightPath: string, highlights: Highlight[]): string;
//# sourceMappingURL=highlightUtils.d.ts.map