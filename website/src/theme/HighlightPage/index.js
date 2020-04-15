import React from 'react';

import Avatar from '@site/src/components/Avatar';
import CTA from '@site/src/components/CTA';
import Layout from '@theme/Layout';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import PagePaginator from '@theme/PagePaginator';
import Tags from '@site/src/components/Tags';
import TimeAgo from 'timeago-react';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
import styles from './styles.module.css';

function prTags(numbers) {
  return numbers.map(number => ({
    enriched: true,
    label: <><i className="feather icon-git-pull-request"></i> {number}</>,
    permalink: `https://github.com/timberio/vector/pull/${number}`,
    style: 'secondary'
  }));
}

function HighlightPage(props) {
  const {content: HighlightContents} = props;
  const {frontMatter, metadata} = HighlightContents;
  const {author_github, description, id, pr_numbers: prNumbers, title} = frontMatter;
  const {date: dateString, tags} = metadata;
  const date = new Date(Date.parse(dateString));

  let enrichedTags = enrichTags(tags, 'highlights');
  enrichedTags = enrichedTags.concat(prTags(prNumbers));

  return (
    <Layout title={title} description={`${title}, in minutes, for free`}>
      <article className={styles.blogPost}>
        <header className={classnames('hero', 'domain-bg', 'domain-bg--nodes', styles.header)}>
          <div className={classnames('container', styles.headerContainer)}>
            <div class="hero--avatar">
              <Avatar
                github={author_github}
                size="lg"
                nameSuffix={<> / <TimeAgo pubdate="pubdate" title={dateFormat(date, "mmm dS, yyyy")} datetime={date} /></>}
                rel="author"
                subTitle={false}
                vertical={true} />
            </div>
            <h1>{title}</h1>
            <div class="hero--subtitle">{description}</div>
            <div className="hero--tags">
              <Tags colorProfile="blog" tags={enrichedTags} />
            </div>
          </div>
        </header>
        <div className="container container--xs margin-vert--xl">
          <section className="markdown">
            <MDXProvider components={MDXComponents}><HighlightContents /></MDXProvider>
          </section>
          <section>
            <h2>Like What You See?</h2>
            <CTA />
          </section>
          {(metadata.nextItem || metadata.prevItem) && (
            <div className="margin-vert--xl">
              <PagePaginator
                next={metadata.nextItem}
                previous={metadata.prevItem}
              />
            </div>
          )}
        </div>
      </article>
    </Layout>
  );
}

export default HighlightPage;
