"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const sitemap_1 = __importDefault(require("sitemap"));
function createSitemap(siteConfig, routesPaths, options) {
    const { url: hostname } = siteConfig;
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
        };
    });
    return sitemap_1.default.createSitemap({
        hostname,
        cacheTime: options.cacheTime,
        urls,
    });
}
exports.default = createSitemap;
