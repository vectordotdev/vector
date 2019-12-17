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
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewPost} from '@site/src/exports/newPost';

import './styles.css';

function BlogListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {post_tags: postTags}} = siteConfig.customFields;
  const enrichedTags = enrichTags(postTags);
  const typeTags = enrichedTags.filter(tag => tag.category == 'type');
  const domainTags = enrichedTags.filter(tag => tag.category == 'domain');

  viewedNewPost();

  return (
    <Layout title="Blog" description="Blog">
      <div className="blog-list container">
        <div className="blog-list--filters">
          <a href="/blog/rss.xml" style={{float: 'right', fontSize: '1.5em', marginTop: '0px', marginLeft: '-30px', width: '30px'}}><i className="feather icon-rss"></i></a>
          <h1>The Vector Blog</h1>
          <p>Thoughts on logs, metrics, and all things observability from the <Link to="/community#team">Vector & Timber.io team</Link>.</p>

          <h3>Types</h3>

          <ul className="filters unstyled">
            {typeTags.map((tag, idx) => (
              <li><Link to={tag.permalink + '/'} className="badge badge--rounded badge--pink">{tag.value}</Link></li>
            ))}
          </ul>

          <h3>Domains</h3>
          
          <ul className="filters unstyled">
            {domainTags.map((tag, idx) => (
              <li><Link to={tag.permalink + '/'} className="badge badge--rounded badge--blue">{tag.value}</Link></li>
            ))}
          </ul>

          <MailingListForm block={true} buttonClass="highlight" />
        </div>
        <div className="blog-list--items">
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
    </Layout>
  );
}

export default BlogListPage;
