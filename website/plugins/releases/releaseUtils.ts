import {PluginOptions, Release} from './types';
import {LoadContext} from '@docusaurus/types';

import _ from 'lodash';
import fs from 'fs-extra';
import globby from 'globby';
import path from 'path';
import {parse, normalizeUrl, aliasedSitePath} from '@docusaurus/utils';
import semver from 'semver';

export function truncate(fileString: string, truncateMarker: RegExp) {
  return fileString.split(truncateMarker, 1).shift()!;
}

export async function generateReleases(
  releaseDir: string,
  {siteConfig, siteDir}: LoadContext,
  options: PluginOptions,
) {
  const {include, routeBasePath, truncateMarker} = options;

  if (!fs.existsSync(releaseDir)) {
    return [];
  }

  const {baseUrl = ''} = siteConfig;
  const releaseFiles = await globby(include, {
    cwd: releaseDir,
  });

  const releases: Release[] = [];

  await Promise.all(
    releaseFiles.map(async (relativeSource: string) => {
      const source = path.join(releaseDir, relativeSource);
      const aliasedSource = aliasedSitePath(source, siteDir);
      const fileString = await fs.readFile(source, 'utf-8');
      const {frontMatter, content, excerpt} = parse(fileString);

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
          permalink: normalizeUrl([
            baseUrl,
            routeBasePath,
            frontMatter.id || version,
          ]),
          source: aliasedSource,
          title: title,
          truncated: truncateMarker?.test(content) || false,
          version: version,
        },
      });
    }),
  );

  return releases.sort((a, b) => semver.compare(a.metadata.version, b.metadata.version));
}

export function linkify(
  fileContent: string,
  siteDir: string,
  releasePath: string,
  releases: Release[],
) {
  let fencedBlock = false;
  const lines = fileContent.split('\n').map(line => {
    if (line.trim().startsWith('```')) {
      fencedBlock = !fencedBlock;
    }

    if (fencedBlock) return line;

    let modifiedLine = line;
    const mdRegex = /(?:(?:\]\()|(?:\]:\s?))(?!https)([^'")\]\s>]+\.mdx?)/g;
    let mdMatch = mdRegex.exec(modifiedLine);

    while (mdMatch !== null) {
      const mdLink = mdMatch[1];
      const aliasedPostSource = `@site/${path.relative(
        siteDir,
        path.resolve(releasePath, mdLink),
      )}`;
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
