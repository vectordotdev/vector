/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import GuideItem from '@theme/GuideItem';
import GuideListPaginator from '@theme/GuideListPaginator';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MailingListForm from '@site/src/components/MailingListForm';

import {enrichTags} from '@site/src/exports/tags';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewPost} from '@site/src/exports/newPost';

import './styles.css';

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {title: siteTitle}} = context;
  const {metadata: {post_tags: postTags}} = siteConfig.customFields;
  const enrichedTags = enrichTags(postTags);
  const typeTags = enrichedTags.filter(tag => tag.category == 'type');
  const domainTags = enrichedTags.filter(tag => tag.category == 'domain');
  const isGuideOnlyMode = metadata.permalink === '/';
  const title = isGuideOnlyMode ? siteTitle : 'Guide';

  viewedNewPost();

  return (
    <Layout title={title} description="Guide">
      <div className="guide-list container">
        <div className="guide-list--filters">
          <a href="/guide/rss.xml" style={{float: 'right', fontSize: '1.5em', marginTop: '0px', marginLeft: '-30px', width: '30px'}}><i className="feather icon-rss"></i></a>
          <h1>The Vector Guide</h1>
          <p>Thoughts on monitoring and observability from the <Link to="/community/#team">Vector & Timber.io team</Link>.</p>

          <h3>Types</h3>

          <ul className="filters unstyled">
            {typeTags.map((tag, idx) => (
              <li key={idx}><Link to={tag.permalink + '/'} className="badge badge--rounded badge--pink">{tag.value}</Link></li>
            ))}
          </ul>

          <h3>Domains</h3>

          <ul className="filters unstyled">
            {domainTags.map((tag, idx) => (
              <li key={idx}><Link to={tag.permalink + '/'} className="badge badge--rounded badge--blue">{tag.value}</Link></li>
            ))}
          </ul>

          <MailingListForm block={true} buttonClass="highlight" />
        </div>
        <div className="guide-list--items">
          {items.map(({content: GuideContent}) => (
            <GuideItem
              key={GuideContent.metadata.permalink}
              frontMatter={GuideContent.frontMatter}
              metadata={GuideContent.metadata}
              truncated={GuideContent.metadata.truncated}>
              <GuideContent />
            </GuideItem>
          ))}
          <GuideListPaginator metadata={metadata} />
        </div>
      </div>
    </Layout>
  );
}

export default GuideListPage;
