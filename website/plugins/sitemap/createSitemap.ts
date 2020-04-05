import sitemap, {SitemapItemOptions} from 'sitemap';
import {PluginOptions} from './types';
import {DocusaurusConfig} from '@docusaurus/types';

export default function createSitemap(
  siteConfig: DocusaurusConfig,
  routesPaths: string[],
  options: PluginOptions,
) {
  const {url: hostname} = siteConfig;
  if (!hostname) {
    throw new Error('Url in docusaurus.config.js cannot be empty/undefined');
  }

  const urls = routesPaths.
    filter(routePath => !routePath.includes("404.html")).
    map(routePath => {
      let url = routePath.endsWith('/') ? routePath : (routePath + '/');

      return {
        url: url,
        changefreq: options.changefreq,
        priority: options.priority,
      } as SitemapItemOptions;
    });

  return sitemap.createSitemap({
    hostname,
    cacheTime: options.cacheTime,
    urls,
  });
}
