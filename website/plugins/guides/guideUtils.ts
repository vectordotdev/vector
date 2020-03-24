/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import fs from 'fs-extra';
import globby from 'globby';
import path from 'path';
import {PluginOptions, Guide} from './types';
import {parse, normalizeUrl, aliasedSitePath} from '@docusaurus/utils';
import {LoadContext} from '@docusaurus/types';

export function truncate(fileString: string, truncateMarker: RegExp) {
  return fileString.split(truncateMarker, 1).shift()!;
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
      const fileString = await fs.readFile(source, 'utf-8');
      const {frontMatter, content, excerpt} = parse(fileString);

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
          permalink: normalizeUrl([
            baseUrl,
            routeBasePath,
            frontMatter.id || linkName,
          ]),
          sort: frontMatter.sort,
          source: aliasedSource,
          tags: (frontMatter.tags || []).concat(category),
          title: frontMatter.title,
          truncated: truncateMarker?.test(content) || false,
        },
      });
    }),
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
