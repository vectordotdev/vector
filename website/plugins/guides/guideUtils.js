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
const utils_1 = require("@docusaurus/utils");
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
        const { frontMatter, content, excerpt } = utils_1.parse(fileString);
        if (frontMatter.draft && process.env.NODE_ENV === 'production') {
            return;
        }
        let category = relativeSource.split('/')[0];
        let linkName = relativeSource.replace(/\.mdx?$/, '');
        frontMatter.title = frontMatter.title || linkName;
        guides.push({
            id: frontMatter.id || frontMatter.title,
            metadata: {
                category: category,
                description: frontMatter.description || excerpt,
                permalink: utils_1.normalizeUrl([
                    baseUrl,
                    routeBasePath,
                    frontMatter.id || linkName,
                ]),
                sort: frontMatter.sort,
                source: aliasedSource,
                tags: (frontMatter.tags || []).concat(category),
                title: frontMatter.title,
                truncated: (truncateMarker === null || truncateMarker === void 0 ? void 0 : truncateMarker.test(content)) || false,
            },
        });
    }));
    guides.sort((a, b) => b.metadata.sort - (a.metadata.sort || 0));
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
