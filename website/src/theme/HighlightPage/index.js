import React from 'react';

import Avatar from '@site/src/components/Avatar';
import CTA from '@site/src/components/CTA';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import PagePaginator from '@theme/PagePaginator';
import Tags from '@site/src/components/Tags';
import TimeAgo from 'timeago-react';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
import styles from './styles.module.css';

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

function prTags(numbers) {
  return numbers.map(number => ({
    enriched: true,
    label: <><i className="feather icon-git-pull-request"></i> {number}</>,
    permalink: `https://github.com/timberio/vector/pull/${number}`,
    style: 'secondary'
  }));
}

function GetText({release}) {
  if (release == 'nightly') {
    return <>This change will be in Vector's next stable release. If you prefer not to wait, you can get this change by <Link to="/releases/nightly/download/">downloading a nightly release</Link>. Please note, nightly releases contain bleeding edge changes that may be unstable.</>;
  } else {
    return <p>This change was made available in <Link to={`/releases/${release}/`}>{release}</Link>. You can get this change by <Link to="/releases/latest/download/">downloading the latest stable release</Link>.</p>;
  }
}

function HighlightPage(props) {
  const {content: HighlightContents} = props;
  const {frontMatter, metadata} = HighlightContents;
  const {author_github, description, id, pr_numbers: prNumbers, release, title} = frontMatter;
  const {date: dateString, tags} = metadata;
  const date = new Date(Date.parse(dateString));
  const formattedDate = dateFormat(date, "mmm dS, yyyy");

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
                nameSuffix={<> / {formattedDate} / <TimeAgo pubdate="pubdate" title={formattedDate} datetime={date} /></>}
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
            <AnchoredH2 id="get">Get This Change</AnchoredH2>

            <GetText release={release} />
          </section>
          <section>
            <AnchoredH2 id="cta">Like What You See?</AnchoredH2>
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
