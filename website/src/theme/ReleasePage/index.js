import React from 'react';

import Alert from '@site/src/components/Alert';
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

  if (breaking) {
    warnings.push(<li>This release contains <a href="#breaking-changes">breaking changes</a>. Please review and follow the <a href="#breaking-change-highlights">upgrade guides</a>.</li>)
  }

  //
  // Render
  //

  return (
    <Layout title={title} description={description}>
      <header className="hero hero--clean hero--flush">
        <div className="container">
          <DownloadDiagram />
          <h1 className={styles.header}>{title}</h1>
          <div class="hero--subtitle">{codename}</div>
          <div class="hero--subsubtitle">
            {formattedDate} / <TimeAgo pubdate="pubdate" title={formattedDate} datetime={date} />
          </div>
          <div class="hero--buttons margin-vert--md">
            <Link to={`/releases/${version}/download/`} className="button button--highlight">
              <i className="feather icon-download"></i> download
            </Link>
          </div>
        </div>
      </header>
      <main className={classnames('container', 'container--xs')}>
        <div class={styles.article}>
          {warnings.length > 0 && <Alert icon={false} fill={true} type="danger" className="list--warnings margin-bottom--lg">
            <ul>{warnings}</ul>
          </Alert>}
          {warnings.length == 0 && <Alert fill={true} icon="check-circle" type="primary" className="margin-bottom--lg">
            This release can be cleanly upgraded. There no breaking changes or special upgrade actions.
          </Alert>}
          <section className="markdown">
            <MDXProvider components={MDXComponents}><ReleaseContents /></MDXProvider>
          </section>
          <section>
            <h2>Like What You See?</h2>

            <div className="row">
              <div className="col">
                <a href="https://twitter.com/vectordotdev" target="_blank" className={classnames('panel', styles.mailingList)} style={{textAlign: 'center'}}>
                  <div className="panel--icon">
                    <i className="feather icon-twitter" title="Twitter"></i>
                  </div>
                  <div className="panel--title">Follow @vectordotdev</div>
                  <div className="panel--description">Get real-time updates!</div>
                </a>
              </div>
              <div className="col">
                <a href="https://github.com/timberio/vector" target="_blank" className="panel text--center">
                  <div className="panel--icon">
                    <i className="feather icon-github"></i>
                  </div>
                  <div className="panel--title">Star timberio/vector</div>
                  <div className="panel--description">Star the repo to support us.</div>
                </a>
              </div>
            </div>
          </section>
        </div>
        {(metadata.nextItem || metadata.prevItem) && (
          <div className="margin-bottom--lg">
            <PagePaginator
              next={metadata.nextItem}
              previous={metadata.prevItem}
            />
          </div>
        )}
      </main>
    </Layout>
  );
}

export default ReleasePage;
