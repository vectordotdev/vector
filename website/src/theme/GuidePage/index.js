import React from 'react';

import Layout from '@theme/Layout';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Jump from '@site/src/components/Jump';
import useTOCHighlight from '@theme/hooks/useTOCHighlight';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {fetchNewPost, viewedNewPost} from '@site/src/exports/newPost';
import readingTime from 'reading-time';
import styles from './styles.module.css';

const LINK_CLASS_NAME = 'contents__link';
const ACTIVE_LINK_CLASS_NAME = 'contents__link--active';
const TOP_OFFSET = 100;

function DocTOC({headings}) {
  useTOCHighlight(LINK_CLASS_NAME, ACTIVE_LINK_CLASS_NAME, TOP_OFFSET);
  return (
    <div className="col col--2">
      <div className={styles.tableOfContents}>
        <Headings headings={headings} />
      </div>
    </div>
  );
}

/* eslint-disable jsx-a11y/control-has-associated-label */
function Headings({headings, isChild}) {
  if (!headings.length) return null;
  return (
    <ul className={isChild ? '' : 'contents contents__left-border'}>
      {headings.map(heading => (
        <li key={heading.id}>
          <a
            href={`#${heading.id}`}
            className={LINK_CLASS_NAME}
            dangerouslySetInnerHTML={{__html: heading.value}}
          />
          <Headings isChild headings={heading.children} />
        </li>
      ))}
    </ul>
  );
}

function GuidePage(props) {
  const {content: GuideContents} = props;
  const {frontMatter, metadata} = GuideContents;
  const {id, title} = frontMatter;
  const {date: dateString, keywords} = metadata;
  const readingStats = readingTime(GuideContents.toString());
  const date = new Date(Date.parse(dateString));
  const newPost = fetchNewPost();

  if (newPost && newPost.id == id) {
    viewedNewPost();
  }

  return (
    <Layout title={metadata.title} description={metadata.description} keywords={metadata.keywords}>
      <div className="container">
        <div className="row">
          <div className="col">
            <div className={styles.guideContainer}>
              <article className={styles.guidePost}>
                <header className={classnames('hero', 'domain-bg', styles.header)}>
                  <div className={classnames('container', styles.headerContainer)}>
                    <h1>{title}</h1>
                    <><time pubdate="pubdate" dateTime={date.toISOString()}>{dateFormat(date, "mmm dS")}</time> / {readingStats.text}</>
                  </div>
                </header>
                <div className="container container--narrow container--bleed margin-vert--xl">
                  <section className="markdown">
                    <MDXProvider components={MDXComponents}><GuideContents /></MDXProvider>
                  </section>
                  <Jump to="/guides">Find another guide</Jump>
                </div>
              </article>
            </div>
          </div>
          {GuideContents.rightToc && (
            <DocTOC headings={GuideContents.rightToc} />
          )}
        </div>
      </div>
    </Layout>
  );
}

export default GuidePage;
