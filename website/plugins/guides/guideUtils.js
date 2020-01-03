/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

const fs = require('fs-extra');
const globby = require('globby');
const path = require('path');
const {parse, normalizeUrl, aliasedSitePath} = require('@docusaurus/utils');

module.exports = {
  truncate: truncate,
  generateGuidePosts: generateGuidePosts,
};

function truncate(fileString, truncateMarker) {
  return fileString.split(truncateMarker, 1).shift() || '';
}

// YYYY-MM-DD-{name}.mdx?
// prefer named capture, but old Node version does not support.
const FILENAME_PATTERN = /^(\d{4}-\d{1,2}-\d{1,2})-?(.*?).mdx?$/;

function toUrl({date, link}) {
  return `${date
    .toISOString()
    .substring(0, '2019-01-01'.length)
    .replace(/-/g, '/')}/${link}`;
}

async function generateGuidePosts(
  guideDir,
  {siteConfig, siteDir},
  options,
) {
  const {include, routeBasePath} = options;

  if (!fs.existsSync(guideDir)) {
    return null;
  }

  const {baseUrl = ''} = siteConfig;
  const guideFiles = await globby(include, {
    cwd: guideDir,
  });

  const guidePosts = [];

  await Promise.all(
    guideFiles.map(async (relativeSource) => {
      const source = path.join(guideDir, relativeSource);
      const aliasedSource = aliasedSitePath(source, siteDir);
      const guideFileName = path.basename(relativeSource);

      const fileString = await fs.readFile(source, 'utf-8');
      const {frontMatter, excerpt} = parse(fileString);

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

      guidePosts.push({
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
          featured: frontMatter.featured || false,
          keywords: frontMatter.keywords,
          title: frontMatter.title,
        },
      });
    }),
  );

  guidePosts.sort(
    (a, b) => b.metadata.date.getTime() - a.metadata.date.getTime(),
  );

  return guidePosts;
}
