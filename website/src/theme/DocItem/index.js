/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import Head from '@docusaurus/Head';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import DocPaginator from '@theme/DocPaginator';

import styles from './styles.module.css';

function Headings({headings, isChild}) {
  if (!headings.length) return null;
  return (
    <ul className={isChild ? '' : 'contents'}>
      {headings.map(heading => (
        <li key={heading.id}>
          <a href={`#${heading.id}`} className="contents__link">
            {heading.value}
          </a>
          <Headings isChild headings={heading.children} />
        </li>
      ))}
    </ul>
  );
}

function DocItem(props) {
  const {siteConfig = {}} = useDocusaurusContext();
  const {url: siteUrl} = siteConfig;
  const {metadata, content: DocContent} = props;
  const {
    description,
    title,
    permalink,
    image: metaImage,
    editUrl,
    lastUpdatedAt,
    lastUpdatedBy,
    keywords,
  } = metadata;

  const metaImageUrl = siteUrl + useBaseUrl(metaImage);

  return (
    <div>
      <Head>
        {title && <title>{title}</title>}
        {description && <meta name="description" content={description} />}
        {description && (
          <meta property="og:description" content={description} />
        )}
        {keywords && keywords.length && (
          <meta name="keywords" content={keywords.join(',')} />
        )}
        {metaImage && <meta property="og:image" content={metaImageUrl} />}
        {metaImage && <meta property="twitter:image" content={metaImageUrl} />}
        {metaImage && (
          <meta name="twitter:image:alt" content={`Image for ${title}`} />
        )}
        {permalink && <meta property="og:url" content={siteUrl + permalink} />}
      </Head>
      <div className="padding-vert--lg">
        <div className="container">
          <div className="row">
            <div className="col">
              <div className={styles.docItemContainer}>
                {!metadata.hide_title && (
                  <header>
                    <h1 className={styles.docTitle}>{metadata.title}</h1>
                  </header>
                )}
                <article>
                  <div className="markdown">
                    <DocContent />
                  </div>
                </article>
                <div className="margin-vert--lg">
                  <DocPaginator metadata={metadata} />
                </div>
              </div>
            </div>
            {DocContent.rightToc && (
              <div className="col col--3">
                <div className={styles.tableOfContents}>
                  <div className="section">
                    <div className="title">Status</div>
                    <div className="status text--primary"><i className="feather icon-check"></i> prod-ready</div>
                    <div className="status text--warning"><i className="feather icon-shield"></i> best-effort</div>
                  </div>
                  <div className="section">
                    <div className="title">Contents</div>
                    <Headings headings={DocContent.rightToc} />
                  </div>
                  <div className="section">
                    <div className="title">Resources</div>
                    <ul className="contents">
                      {editUrl && (<li><a href={editUrl} className="contents__link"><i className="feather icon-edit-1"></i> Edit this page</a></li>)}
                      <li><a href="#" className="contents__link"><i className="feather icon-message-circle"></i> View Issues</a></li>
                      <li><a href="#" className="contents__link"><i className="feather icon-github"></i> View Source</a></li>
                    </ul>
                  </div>
                  <div className="section">
                    Last edit at 12/12/12 by Ben
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default DocItem;
