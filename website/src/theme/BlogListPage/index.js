/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import BlogPostItem from '@theme/BlogPostItem';
import BlogListPaginator from '@theme/BlogListPaginator';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MailingListForm from '@site/src/components/MailingListForm';

import {enrichTags} from '@site/src/exports/tags';
import GithubSlugger from 'github-slugger';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function BlogListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {post_tags: postTags}} = siteConfig.customFields;

  const slugger = new GithubSlugger();
  const tags = postTags.sort().map(postTag => ({label: postTag, permalink: `/blog/tags/${slugger.slug(postTag)}`}));
  const enrichedTags = enrichTags(tags);

  const typeTags = enrichedTags.filter(tag => tag.category == 'type');
  const domainTags = enrichedTags.filter(tag => tag.category == 'domain');

  return (
    <Layout title="Blog" description="Blog">
      <div className="container margin-vert--xl">
        <div className="row">
          <div className="col col--4 blog-list-filters">
            <h1>The Vector Blog</h1>
            <p>Thoughts logs, metrics, and all things observability from the <a href="/">Vector team</a>.</p>

            <h3>Types</h3>

            <ul className="filters unstyled">
              {typeTags.map((tag, idx) => (
                <li><Link to={tag.permalink} className="badge badge--rounded badge--pink">{tag.value}</Link></li>
              ))}
            </ul>

            <h3>Domains</h3>
            
            <ul className="filters unstyled">
              {domainTags.map((tag, idx) => (
                <li><Link to={tag.permalink} className="badge badge--rounded badge--blue">{tag.value}</Link></li>
              ))}
            </ul>

            <hr />

            <MailingListForm block={true} />
          </div>
          <div className="col col--8">
            {items.map(({content: BlogPostContent}) => (
              <BlogPostItem
                key={BlogPostContent.metadata.permalink}
                frontMatter={BlogPostContent.frontMatter}
                metadata={BlogPostContent.metadata}
                truncated>
                <BlogPostContent />
              </BlogPostItem>
            ))}
            <BlogListPaginator metadata={metadata} />
          </div>
        </div>
      </div>
    </Layout>
  );
}

export default BlogListPage;
