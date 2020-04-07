"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const lodash_1 = __importDefault(require("lodash"));
const fs_extra_1 = __importDefault(require("fs-extra"));
const globby_1 = __importDefault(require("globby"));
const humanize_string_1 = __importDefault(require("humanize-string"));
const path_1 = __importDefault(require("path"));
const utils_1 = require("@docusaurus/utils");
const reading_time_1 = __importDefault(require("reading-time"));
const titleize_1 = __importDefault(require("titleize"));
function truncate(fileString, truncateMarker) {
    return fileString.split(truncateMarker, 1).shift();
}
exports.truncate = truncate;
async function generateGuides(guideDir, { siteConfig, siteDir }, options) {
    const { include, routeBasePath, truncateMarker } = options;
    if (!fs_extra_1.default.existsSync(guideDir)) {
        return [];
    }
    const { baseUrl = '' } = siteConfig;
    const guideFiles = await globby_1.default(include, {
        cwd: guideDir,
    });
    const guides = [];
    await Promise.all(guideFiles.map(async (relativeSource) => {
        const source = path_1.default.join(guideDir, relativeSource);
        const aliasedSource = utils_1.aliasedSitePath(source, siteDir);
        const fileString = await fs_extra_1.default.readFile(source, 'utf-8');
        const readingStats = reading_time_1.default(fileString);
        const { frontMatter, content, excerpt } = utils_1.parse(fileString);
        if (frontMatter.draft && process.env.NODE_ENV === 'production') {
            return;
        }
        let categoryParts = relativeSource.split('/').slice(0, -1);
        let categories = [];
        while (categoryParts.length > 0) {
            let name = categoryParts[categoryParts.length - 1];
            let title = titleize_1.default(humanize_string_1.default(name));
            let description = null;
            switch (name) {
                case 'advanced':
                    description = 'Go beyond the basics, become a Vector pro, and extract the full potential of Vector.';
                    break;
                case 'getting-started':
                    description = 'Take Vector from zero to production in under 10 minutes.';
                    break;
                case 'integrate':
                    description = 'Simple step-by-step integration guides.';
                    break;
            }
            categories.unshift({
                name: name,
                title: title,
                description: description,
                permalink: utils_1.normalizeUrl([baseUrl, routeBasePath, categoryParts.join('/')])
            });
            categoryParts.pop();
        }
        let linkName = relativeSource.replace(/\.mdx?$/, '');
        let seriesPosition = frontMatter.series_position;
        let tags = frontMatter.tags || [];
        let title = frontMatter.title || linkName;
        let coverLabel = frontMatter.cover_label || title;
        guides.push({
            id: frontMatter.id || frontMatter.title,
            metadata: {
                categories: categories,
                coverLabel: coverLabel,
                description: frontMatter.description || excerpt,
                permalink: utils_1.normalizeUrl([
                    baseUrl,
                    routeBasePath,
                    frontMatter.id || linkName,
                ]),
                readingTime: readingStats.text,
                seriesPosition: seriesPosition,
                sort: frontMatter.sort,
                source: aliasedSource,
                tags: tags,
                title: title,
                truncated: (truncateMarker === null || truncateMarker === void 0 ? void 0 : truncateMarker.test(content)) || false,
            },
        });
    }));
    return lodash_1.default.sortBy(guides, [
        ((guide) => {
            let categories = guide.metadata.categories;
            if (categories[0].name == 'getting-started') {
                return ['AA'].concat(categories.map(category => category.name).slice(1));
            }
            else {
                return categories;
            }
        }),
        'metadata.seriesPosition',
        ((guide) => guide.metadata.coverLabel.toLowerCase())
    ]);
}
exports.generateGuides = generateGuides;
function linkify(fileContent, siteDir, guidePath, guides) {
    let fencedBlock = false;
    const lines = fileContent.split('\n').map(line => {
        if (line.trim().startsWith('```')) {
            fencedBlock = !fencedBlock;
        }
        if (fencedBlock)
            return line;
        let modifiedLine = line;
        const mdRegex = /(?:(?:\]\()|(?:\]:\s?))(?!https)([^'")\]\s>]+\.mdx?)/g;
        let mdMatch = mdRegex.exec(modifiedLine);
        while (mdMatch !== null) {
            const mdLink = mdMatch[1];
            const aliasedPostSource = `@site/${path_1.default.relative(siteDir, path_1.default.resolve(guidePath, mdLink))}`;
            let guidePermalink = null;
            guides.forEach(guide => {
                if (guide.metadata.source === aliasedPostSource) {
                    guidePermalink = guide.metadata.permalink;
                }
            });
            if (guidePermalink) {
                modifiedLine = modifiedLine.replace(mdLink, guidePermalink);
            }
            mdMatch = mdRegex.exec(modifiedLine);
        }
        return modifiedLine;
    });
    return lines.join('\n');
}
exports.linkify = linkify;
