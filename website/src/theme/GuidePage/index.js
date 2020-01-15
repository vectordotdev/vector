import React from 'react';

import Layout from '@theme/Layout';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Jump from '@site/src/components/Jump';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {fetchNewPost, viewedNewPost} from '@site/src/exports/newPost';
import readingTime from 'reading-time';
import styles from './styles.module.css';

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
      <article className={styles.blogPost}>
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
    </Layout>
  );
}

export default GuidePage;
