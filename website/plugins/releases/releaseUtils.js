"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_extra_1 = __importDefault(require("fs-extra"));
const globby_1 = __importDefault(require("globby"));
const path_1 = __importDefault(require("path"));
const utils_1 = require("@docusaurus/utils");
const semver_1 = __importDefault(require("semver"));
function truncate(fileString, truncateMarker) {
    return fileString.split(truncateMarker, 1).shift();
}
exports.truncate = truncate;
async function generateReleases(releaseDir, { siteConfig, siteDir }, options) {
    const { include, routeBasePath, truncateMarker } = options;
    if (!fs_extra_1.default.existsSync(releaseDir)) {
        return [];
    }
    const { baseUrl = '' } = siteConfig;
    const releaseFiles = await globby_1.default(include, {
        cwd: releaseDir,
    });
    const releases = [];
    await Promise.all(releaseFiles.map(async (relativeSource) => {
        const source = path_1.default.join(releaseDir, relativeSource);
        const aliasedSource = utils_1.aliasedSitePath(source, siteDir);
        const fileString = await fs_extra_1.default.readFile(source, 'utf-8');
        const { frontMatter, content, excerpt } = utils_1.parse(fileString);
        if (frontMatter.draft && process.env.NODE_ENV === 'production') {
            return;
        }
        let date = new Date(frontMatter.date ? Date.parse(frontMatter.date) : Date.now());
        let description = frontMatter.description || excerpt;
        let version = relativeSource.replace(/\.mdx?$/, '');
        let title = frontMatter.title || version;
        releases.push({
            id: frontMatter.id || frontMatter.title,
            metadata: {
                date: date,
                description: description,
                permalink: utils_1.normalizeUrl([
                    baseUrl,
                    routeBasePath,
                    frontMatter.id || version,
                ]),
                source: aliasedSource,
                title: title,
                truncated: (truncateMarker === null || truncateMarker === void 0 ? void 0 : truncateMarker.test(content)) || false,
                version: version,
            },
        });
    }));
    return releases.sort((a, b) => semver_1.default.compare(a.metadata.version, b.metadata.version));
}
exports.generateReleases = generateReleases;
function linkify(fileContent, siteDir, releasePath, releases) {
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
            const aliasedPostSource = `@site/${path_1.default.relative(siteDir, path_1.default.resolve(releasePath, mdLink))}`;
            let releasePermalink = null;
            releases.forEach(release => {
                if (release.metadata.source === aliasedPostSource) {
                    releasePermalink = release.metadata.permalink;
                }
            });
            if (releasePermalink) {
                modifiedLine = modifiedLine.replace(mdLink, releasePermalink);
            }
            mdMatch = mdRegex.exec(modifiedLine);
        }
        return modifiedLine;
    });
    return lines.join('\n');
}
exports.linkify = linkify;
