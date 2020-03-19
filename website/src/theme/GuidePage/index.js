import React from 'react';

import CodeBlock from '@theme/CodeBlock';
import Heading from '@theme/Heading';
import InstallationCommand from '@site/src/components/InstallationCommand';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import readingTime from 'reading-time';
import styles from './styles.module.css';
import useTOCHighlight from '@theme/hooks/useTOCHighlight';

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

const LINK_CLASS_NAME = 'contents__link';
const ACTIVE_LINK_CLASS_NAME = 'contents__link--active';
const TOP_OFFSET = 100;

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
  const {category, id, title} = frontMatter;
  const {date: dateString, tags} = metadata;
  const readingStats = readingTime(GuideContents.toString());
  const date = new Date(Date.parse(dateString));

  return (
    <Layout title="Collect Docker Logs & Send Them Anywhere" description="Collect Docker logs in minutes, for free. Quickly collect Docker logs and metrics and send them to one or more destinations.">
      <header className="hero domain-bg domain-bg--platforms">
        <div className="container">
          <div className="component-icons">
            <div className="icon panel">
              <img src="/img/logos/docker.png" alt="Docker" />
            </div>
            <a href="#" className="icon panel" title="Select a destination">
              <i className="feather icon-plus"></i>
            </a>
          </div>
          <h1>{title}</h1>
          <div className={styles.credit}>Written, with <i className="feather icon-heart"></i>, by the <Link to="/community/#team">Vector team</Link>, last updated March 22, 2020</div>
          <Tags colorProfile="guides" tags={tags} />
        </div>
      </header>
      <main className="container container--narrow margin-vert--xl">
        <section className="markdown align-text-edges">
          <MDXProvider components={MDXComponents}><GuideContents /></MDXProvider>
        </section>
      </main>
    </Layout>
  );
}

export default GuidePage;
