"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = __importDefault(require("fs"));
const path_1 = __importDefault(require("path"));
const createSitemap_1 = __importDefault(require("./createSitemap"));
const DEFAULT_OPTIONS = {
    cacheTime: 600 * 1000,
    changefreq: 'weekly',
    priority: 0.5,
};
function pluginSitemap(_context, opts) {
    const options = Object.assign(Object.assign({}, DEFAULT_OPTIONS), opts);
    return {
        name: 'docusaurus-plugin-sitemap',
        async postBuild({ siteConfig, routesPaths, outDir }) {
            // Generate sitemap.
            const generatedSitemap = createSitemap_1.default(siteConfig, routesPaths, options).toString();
            // Write sitemap file.
            const sitemapPath = path_1.default.join(outDir, 'sitemap.xml');
            try {
                fs_1.default.writeFileSync(sitemapPath, generatedSitemap);
            }
            catch (err) {
                throw new Error(`Sitemap error: ${err}`);
            }
        },
    };
}
exports.default = pluginSitemap;
