import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Layout from '@theme/Layout';
import BlogPostItem from '@theme/BlogPostItem';
import BlogPostPaginator from '@theme/BlogPostPaginator';
import MailingListForm from '@site/src/components/MailingListForm';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
import {fetchNewPost, viewedNewPost} from '@site/src/exports/newPost';
import readingTime from 'reading-time';
import styles from './styles.module.css';

function BlogPostPage(props) {
  const {content: BlogPostContents} = props;
  const {frontMatter, metadata} = BlogPostContents;
  const {author_github, id, title} = frontMatter;
  const {date: dateString, tags} = metadata;
  const readingStats = readingTime(BlogPostContents.toString());
  const date = new Date(Date.parse(dateString));
  const domainTag = enrichTags(tags).find(tag => tag.category == 'domain');
  const domain = domainTag ? domainTag.value : null;
  const newPost = fetchNewPost();

  if (newPost && newPost.id == id) {
    viewedNewPost();
  }

  return (
    <Layout title={metadata.title} description={metadata.description}>
      <article className={styles.blogPost}>
        <header className={classnames('hero', 'domain-bg', `domain-bg--${domain}`, styles.header)}>
          <div className={classnames('container', styles.headerContainer)}>
            <Avatar github={author_github} size="lg" nameSuffix={<> / <time pubdate="pubdate" dateTime={date.toISOString()}>{dateFormat(date, "mmm dS")}</time> / {readingStats.text}</>} rel="author" subTitle={false} vertical={true} />
            <h1>{title}</h1>
            <div className={styles.headerTags}>
              <Tags tags={tags} />
            </div>
          </div>
        </header>
        <div className="container container--narrow margin-vert--xl">
          <section className="markdown align-text-edges dropcap">
            <MDXProvider components={MDXComponents}><BlogPostContents /></MDXProvider>
          </section>
          <section className={classnames('panel', styles.mailingList)} style={{textAlign: 'center'}}>
            <div className={styles.mailingListTitle}>
              <i className="feather icon-mail"></i> Vector In Your Inbox!
            </div>
            <p>
              One email on the 1st of the month. No spam, ever.
            </p>
            <MailingListForm center={true} description={false} size="lg" />
          </section>
          {(metadata.nextItem || metadata.prevItem) && (
            <div className="margin-vert--xl">
              <BlogPostPaginator
                nextItem={metadata.nextItem}
                prevItem={metadata.prevItem}
              />
            </div>
          )}
        </div>
      </article>
    </Layout>
  );
}

export default BlogPostPage;
