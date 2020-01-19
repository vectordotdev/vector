"use strict";
/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_extra_1 = __importDefault(require("fs-extra"));
const globby_1 = __importDefault(require("globby"));
const path_1 = __importDefault(require("path"));
const feed_1 = require("feed");
const utils_1 = require("@docusaurus/utils");
function truncate(fileString, truncateMarker) {
    return fileString.split(truncateMarker, 1).shift();
}
exports.truncate = truncate;
// YYYY-MM-DD-{name}.mdx?
// Prefer named capture, but older Node versions do not support it.
const FILENAME_PATTERN = /^(\d{4}-\d{1,2}-\d{1,2})-?(.*?).mdx?$/;
function toUrl({ date, link }) {
    return `${date
        .toISOString()
        .substring(0, '2019-01-01'.length)
        .replace(/-/g, '/')}/${link}`;
}
async function generateGuideFeed(context, options) {
    if (!options.feedOptions) {
        throw new Error('Invalid options - `feedOptions` is not expected to be null.');
    }
    const { siteDir, siteConfig } = context;
    const contentPath = path_1.default.resolve(siteDir, options.path);
    const guides = await generateGuides(contentPath, context, options);
    if (guides == null) {
        return null;
    }
    const { feedOptions, routeBasePath } = options;
    const { url: siteUrl, title, favicon } = siteConfig;
    const guideBaseUrl = utils_1.normalizeUrl([siteUrl, routeBasePath]);
    const updated = (guides[0] && guides[0].metadata.date) ||
        new Date('2015-10-25T16:29:00.000-07:00');
    const feed = new feed_1.Feed({
        id: guideBaseUrl,
        title: feedOptions.title || `${title} Guide`,
        updated,
        language: feedOptions.language,
        link: guideBaseUrl,
        description: feedOptions.description || `${siteConfig.title} Guide`,
        favicon: utils_1.normalizeUrl([siteUrl, favicon]),
        copyright: feedOptions.copyright,
    });
    guides.forEach(guide => {
        const { id, metadata: { title, permalink, date, description }, } = guide;
        feed.addItem({
            title,
            id: id,
            link: utils_1.normalizeUrl([siteUrl, permalink]),
            date,
            description,
        });
    });
    return feed;
}
exports.generateGuideFeed = generateGuideFeed;
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
        const guideFileName = path_1.default.basename(relativeSource);
        const fileString = await fs_extra_1.default.readFile(source, 'utf-8');
        const { frontMatter, content, excerpt } = utils_1.parse(fileString);
        if (frontMatter.draft && process.env.NODE_ENV === 'production') {
            return;
        }
        let date;
        // Extract date and title from filename.
        const match = guideFileName.match(FILENAME_PATTERN);
        let linkName = guideFileName.replace(/\.mdx?$/, '');
        if (match) {
            const [, dateString, name] = match;
            date = new Date(dateString);
            linkName = name;
        }
        // Prefer user-defined date.
        if (frontMatter.date) {
            date = new Date(frontMatter.date);
        }
        // Use file create time for guide.
        date = date || (await fs_extra_1.default.stat(source)).birthtime;
        frontMatter.title = frontMatter.title || linkName;
        guides.push({
            id: frontMatter.id || frontMatter.title,
            metadata: {
                permalink: utils_1.normalizeUrl([
                    baseUrl,
                    routeBasePath,
                    frontMatter.id || toUrl({ date, link: linkName }),
                ]),
                source: aliasedSource,
                description: frontMatter.description || excerpt,
                date,
                tags: frontMatter.tags,
                title: frontMatter.title,
                truncated: (truncateMarker === null || truncateMarker === void 0 ? void 0 : truncateMarker.test(content)) || false,
            },
        });
    }));
    guides.sort((a, b) => b.metadata.date.getTime() - a.metadata.date.getTime());
    return guides;
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
