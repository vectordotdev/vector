import { PluginOptions } from './types';
import { LoadContext, Props } from '@docusaurus/types';
export default function pluginSitemap(_context: LoadContext, opts: Partial<PluginOptions>): {
    name: string;
    postBuild({ siteConfig, routesPaths, outDir }: Props): Promise<void>;
};
//# sourceMappingURL=index.d.ts.map