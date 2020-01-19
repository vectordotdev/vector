/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import fs from 'fs-extra';
import globby from 'globby';
import path from 'path';
import {Feed} from 'feed';
import {PluginOptions, Guide, DateLink} from './types';
import {parse, normalizeUrl, aliasedSitePath} from '@docusaurus/utils';
import {LoadContext} from '@docusaurus/types';

export function truncate(fileString: string, truncateMarker: RegExp) {
  return fileString.split(truncateMarker, 1).shift()!;
}

// YYYY-MM-DD-{name}.mdx?
// Prefer named capture, but older Node versions do not support it.
const FILENAME_PATTERN = /^(\d{4}-\d{1,2}-\d{1,2})-?(.*?).mdx?$/;

function toUrl({date, link}: DateLink) {
  return `${date
    .toISOString()
    .substring(0, '2019-01-01'.length)
    .replace(/-/g, '/')}/${link}`;
}

export async function generateGuideFeed(
  context: LoadContext,
  options: PluginOptions,
) {
  if (!options.feedOptions) {
    throw new Error(
      'Invalid options - `feedOptions` is not expected to be null.',
    );
  }
  const {siteDir, siteConfig} = context;
  const contentPath = path.resolve(siteDir, options.path);
  const guides = await generateGuides(contentPath, context, options);
  if (guides == null) {
    return null;
  }

  const {feedOptions, routeBasePath} = options;
  const {url: siteUrl, title, favicon} = siteConfig;
  const guideBaseUrl = normalizeUrl([siteUrl, routeBasePath]);

  const updated =
    (guides[0] && guides[0].metadata.date) ||
    new Date('2015-10-25T16:29:00.000-07:00');

  const feed = new Feed({
    id: guideBaseUrl,
    title: feedOptions.title || `${title} Guide`,
    updated,
    language: feedOptions.language,
    link: guideBaseUrl,
    description: feedOptions.description || `${siteConfig.title} Guide`,
    favicon: normalizeUrl([siteUrl, favicon]),
    copyright: feedOptions.copyright,
  });

  guides.forEach(guide => {
    const {
      id,
      metadata: {title, permalink, date, description},
    } = guide;
    feed.addItem({
      title,
      id: id,
      link: normalizeUrl([siteUrl, permalink]),
      date,
      description,
    });
  });

  return feed;
}

export async function generateGuides(
  guideDir: string,
  {siteConfig, siteDir}: LoadContext,
  options: PluginOptions,
) {
  const {include, routeBasePath, truncateMarker} = options;

  if (!fs.existsSync(guideDir)) {
    return [];
  }

  const {baseUrl = ''} = siteConfig;
  const guideFiles = await globby(include, {
    cwd: guideDir,
  });

  const guides: Guide[] = [];

  await Promise.all(
    guideFiles.map(async (relativeSource: string) => {
      const source = path.join(guideDir, relativeSource);
      const aliasedSource = aliasedSitePath(source, siteDir);
      const guideFileName = path.basename(relativeSource);

      const fileString = await fs.readFile(source, 'utf-8');
      const {frontMatter, content, excerpt} = parse(fileString);

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
      date = date || (await fs.stat(source)).birthtime;
      frontMatter.title = frontMatter.title || linkName;

      guides.push({
        id: frontMatter.id || frontMatter.title,
        metadata: {
          permalink: normalizeUrl([
            baseUrl,
            routeBasePath,
            frontMatter.id || toUrl({date, link: linkName}),
          ]),
          source: aliasedSource,
          description: frontMatter.description || excerpt,
          date,
          tags: frontMatter.tags,
          title: frontMatter.title,
          truncated: truncateMarker?.test(content) || false,
        },
      });
    }),
  );

  guides.sort(
    (a, b) => b.metadata.date.getTime() - a.metadata.date.getTime(),
  );

  return guides;
}

export function linkify(
  fileContent: string,
  siteDir: string,
  guidePath: string,
  guides: Guide[],
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
        path.resolve(guidePath, mdLink),
      )}`;
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
