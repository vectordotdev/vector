import React from 'react';

import Alert from '@site/src/components/Alert';
import CTA from '@site/src/components/CTA';
import DownloadDiagram from '@site/src/components/DownloadDiagram';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import PagePaginator from '@theme/PagePaginator';
import TimeAgo from 'timeago-react';
import Vic from '@site/src/components/Vic';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import pluralize from 'pluralize';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

/* eslint-disable jsx-a11y/control-has-associated-label */
function Headings({headings, isChild}) {
  if (!headings.length) return null;

  // We need to track shown headings because the markdown parser will
  // extract duplicate headings if we're using tabs
  let uniqHeadings = _.uniqBy(headings, (heading => heading.value));

  return (
    <ul className={isChild ? '' : 'contents'}>
      {!isChild && (
        <li>
          <a
            href="#overview"
            className={LINK_CLASS_NAME}>
            Overview
          </a>
        </li>
      )}
      {uniqHeadings.map(heading => (
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

function Badges() {
  const insertions_count = 0;
  const deletions_count = 0;
  const latest = true;
  const last_version = '0.9.0';
  const type = 'initial dev';
  const type_url = 'test';
  const compare_url = 'compare_url';
  let typeClass = 'primary';

  switch(type) {
    case 'initial dev':
      typeClass = 'warning';
      break;
    case 'major':
      typeClass = 'warning';
      break;
  }

  return (
    <>
      {latest ?
        <span
          className="badge badge--primary badge--rounded"
          title="This is the latest (recommended) stable release">
          <i className="feather icon-check"></i> latest
        </span> :
        <a
          href="/releases/latest"
          className="badge badge--warning badge--rounded"
          title="This release is outdated, newer releases are available">
          <i className="feather icon-alert-triangle"></i> outdated
        </a>
      }
      &nbsp;&nbsp;
      <a
        href={type_url}
        target="_blank"
        className={classnames('badge', `badge--${typeClass}`, 'badge--rounded')}
        title={`This is a ${type} release as defined by the semantic versioning spec`}>
        <i className="feather icon-chevrons-up"></i> {type}
      </a>
      &nbsp;&nbsp;
      <a
        href={compare_url}
        target="_blank"
        className="badge badge--primary badge--rounded"
        title={`View the diff since ${last_version}`}>
        +{insertions_count}, -{deletions_count}
      </a>
    </>
  );
}

function Subtitle({subtitle}) {
  if (subtitle) {
    return (
      <>
        <div className="hero--subtitle">{subtitle}</div>
        <div className="hero--subtitle">{subtitle}</div>
      </>
    );
  } else {
    return (
      <div className="hero--subtitle">{subtitle}</div>
    );
  }
}

function ReleasePage(props) {
  //
  // Props
  //

  const {content: ReleaseContents} = props;
  const {frontMatter, metadata} = ReleaseContents;
  const {author_github, codename, id, subtitle, title} = frontMatter;
  const {date: dateString, description, tags, version} = metadata;
  const date = new Date(Date.parse(dateString));
  const formattedDate = dateFormat(date, "mmm dS, yyyy");

  //
  // Context
  //

  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {latest_release: latestRelease, releases}} = siteConfig.customFields;

  //
  // Vars
  //

  const release = releases[version];
  const latest = latestRelease.version != release.version;
  const breaking = release.commits.some(commit => commit.breaking_change);
  let warnings = [];

  if (latest) {
    warnings.push(<li>This is an outdated version. Outdated versions maybe contain bugs. It is recommended to use the <Link to={latestRelease.permalink}>latest version ({latestRelease.version})</Link>.</li>);
  }

  const enhancements = release.commits.filter(commit => commit.type == 'enhancement');
  const features = release.commits.filter(commit => commit.type == 'feat');
  const fixes = release.commits.filter(commit => commit.type == 'fix');

  //
  // Render
  //

  return (
    <Layout title={title} description={description}>
      <main className={styles.main}>
        <header className="hero hero--clean hero--flush">
          <div className="container">
            <DownloadDiagram />
            <h1>{title}</h1>
            <div class="hero--subtitle">{codename}</div>
            <div class="hero--subsubtitle">
              {formattedDate} / <TimeAgo pubdate="pubdate" title={formattedDate} datetime={date} />
            </div>
            <div class="hero--buttons margin-vert--md">
              <Link to={`/releases/${version}/download/`} className="button button--highlight">
                <i className="feather icon-download"></i> download
              </Link>
            </div>
            <div class="hero--toc">
              <ul>
                {release.highlights.length > 0 && <li><a href="#highlights">{pluralize('highlight', release.highlights.length, true)}</a></li>}
                {features.length > 0 && <li><a href="#feat">{pluralize('new feature', features.length, true)}</a></li>}
                {enhancements.length > 0 && <li><a href="#enhancement">{pluralize('enhancement', enhancements.length, true)}</a></li>}
                {fixes.length > 0 && <li><a href="#fix">{pluralize('bug fix', fixes.length, true)}</a></li>}
              </ul>
            </div>
          </div>
        </header>
        <div className={classnames('container', 'container--xs')}>
          <article>
            {warnings.length > 0 && <Alert icon={false} fill={true} type="warning" className="list--icons list--icons--warnings margin-bottom--lg">
              <ul>{warnings}</ul>
            </Alert>}
            <section className="markdown">
              <MDXProvider components={MDXComponents}><ReleaseContents /></MDXProvider>
            </section>
            <section>
              <h2>Like What You See?</h2>

              <CTA />
            </section>
          </article>
          {(metadata.nextItem || metadata.prevItem) && (
            <div className="margin-bottom--lg">
              <PagePaginator
                next={metadata.nextItem}
                previous={metadata.prevItem}
              />
            </div>
          )}
        </div>
      </main>
      <nav className="pagination-controls">
        {metadata.prevItem && <Link to={metadata.prevItem.permalink} className="prev"><i className="feather icon-chevron-left"></i></Link>}
        {metadata.nextItem && <Link to={metadata.nextItem.permalink} className="next"><i className="feather icon-chevron-right"></i></Link>}
      </nav>
    </Layout>
  );
}

export default ReleasePage;
