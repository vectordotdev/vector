import {PluginOptions, Highlight} from './types';
import {LoadContext} from '@docusaurus/types';

import _ from 'lodash';
import fs from 'fs-extra';
import globby from 'globby';
import path from 'path';
import {parse, normalizeUrl, aliasedSitePath} from '@docusaurus/utils';
import readingTime from 'reading-time';

// YYYY-MM-DD-{name}.mdx?
// Prefer named capture, but older Node versions do not support it.
const FILENAME_PATTERN = /^(\d{4}-\d{1,2}-\d{1,2})-?(.*?).mdx?$/;

export function truncate(fileString: string, truncateMarker: RegExp) {
  return fileString.split(truncateMarker, 1).shift()!;
}

export async function generateHighlights(
  highlightDir: string,
  {siteConfig, siteDir}: LoadContext,
  options: PluginOptions,
) {
  const {include, routeBasePath, truncateMarker} = options;

  if (!fs.existsSync(highlightDir)) {
    return [];
  }

  const {baseUrl = ''} = siteConfig;
  const highlightFiles = await globby(include, {
    cwd: highlightDir,
  });

  const highlights: Highlight[] = [];

  await Promise.all(
    highlightFiles.map(async (relativeSource: string) => {
      const source = path.join(highlightDir, relativeSource);
      const aliasedSource = aliasedSitePath(source, siteDir);
      const fileString = await fs.readFile(source, 'utf-8');
      const readingStats = readingTime(fileString);
      const {frontMatter, content, excerpt} = parse(fileString);
      const fileName = path.basename(relativeSource);
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
          permalink: normalizeUrl([
            baseUrl,
            routeBasePath,
            frontMatter.id || linkName,
          ]),
          readingTime: readingStats.text,
          source: aliasedSource,
          tags: tags,
          title: title,
          truncated: truncateMarker?.test(content) || false,
        },
      });
    }),
  );

  return highlights.sort((a, b) => b.metadata.date.getTime() - a.metadata.date.getTime());
}

export function linkify(
  fileContent: string,
  siteDir: string,
  highlightPath: string,
  highlights: Highlight[],
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
        path.resolve(highlightPath, mdLink),
      )}`;
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
