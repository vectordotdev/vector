"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_extra_1 = __importDefault(require("fs-extra"));
const globby_1 = __importDefault(require("globby"));
const path_1 = __importDefault(require("path"));
const utils_1 = require("@docusaurus/utils");
const reading_time_1 = __importDefault(require("reading-time"));
// YYYY-MM-DD-{name}.mdx?
// Prefer named capture, but older Node versions do not support it.
const FILENAME_PATTERN = /^(\d{4}-\d{1,2}-\d{1,2})-?(.*?).mdx?$/;
function truncate(fileString, truncateMarker) {
    return fileString.split(truncateMarker, 1).shift();
}
exports.truncate = truncate;
async function generateHighlights(highlightDir, { siteConfig, siteDir }, options) {
    const { include, routeBasePath, truncateMarker } = options;
    if (!fs_extra_1.default.existsSync(highlightDir)) {
        return [];
    }
    const { baseUrl = '' } = siteConfig;
    const highlightFiles = await globby_1.default(include, {
        cwd: highlightDir,
    });
    const highlights = [];
    await Promise.all(highlightFiles.map(async (relativeSource) => {
        const source = path_1.default.join(highlightDir, relativeSource);
        const aliasedSource = utils_1.aliasedSitePath(source, siteDir);
        const fileString = await fs_extra_1.default.readFile(source, 'utf-8');
        const readingStats = reading_time_1.default(fileString);
        const { frontMatter, content, excerpt } = utils_1.parse(fileString);
        const fileName = path_1.default.basename(relativeSource);
        const fileNameMatch = fileName.match(FILENAME_PATTERN);
        if (frontMatter.draft && process.env.NODE_ENV === 'production') {
            return;
        }
        let date = fileNameMatch ? new Date(fileNameMatch[1]) : new Date(Date.now());
        let description = frontMatter.description || excerpt;
        let id = frontMatter.id || frontMatter.title;
        let linkName = relativeSource.replace(/\.mdx?$/, '');
        let tags = frontMatter.tags || [];
        let title = frontMatter.title || linkName;
        highlights.push({
            id: id,
            metadata: {
                date: date,
                description: description,
                permalink: utils_1.normalizeUrl([
                    baseUrl,
                    routeBasePath,
                    frontMatter.id || linkName,
                ]),
                readingTime: readingStats.text,
                source: aliasedSource,
                tags: tags,
                title: title,
                truncated: (truncateMarker === null || truncateMarker === void 0 ? void 0 : truncateMarker.test(content)) || false,
            },
        });
    }));
    return highlights.sort((a, b) => b.metadata.date.getTime() - a.metadata.date.getTime());
}
exports.generateHighlights = generateHighlights;
function linkify(fileContent, siteDir, highlightPath, highlights) {
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
            const aliasedPostSource = `@site/${path_1.default.relative(siteDir, path_1.default.resolve(highlightPath, mdLink))}`;
            let highlightPermalink = null;
            highlights.forEach(highlight => {
                if (highlight.metadata.source === aliasedPostSource) {
                    highlightPermalink = highlight.metadata.permalink;
                }
            });
            if (highlightPermalink) {
                modifiedLine = modifiedLine.replace(mdLink, highlightPermalink);
            }
            mdMatch = mdRegex.exec(modifiedLine);
        }
        return modifiedLine;
    });
    return lines.join('\n');
}
exports.linkify = linkify;
