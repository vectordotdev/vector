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

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewPost} from '@site/src/exports/newPost';

import './styles.css';

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {title: siteTitle}} = context;
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
