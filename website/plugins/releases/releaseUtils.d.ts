import { PluginOptions, Release } from './types';
import { LoadContext } from '@docusaurus/types';
export declare function truncate(fileString: string, truncateMarker: RegExp): string;
export declare function generateReleases(releaseDir: string, { siteConfig, siteDir }: LoadContext, options: PluginOptions): Promise<Release[]>;
export declare function linkify(fileContent: string, siteDir: string, releasePath: string, releases: Release[]): string;
//# sourceMappingURL=releaseUtils.d.ts.map