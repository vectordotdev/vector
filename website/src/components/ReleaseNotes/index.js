import React, { useEffect } from 'react';

import Alert from '@site/src/components/Alert';
import Avatar from '@site/src/components/Avatar';
import Changelog from '@site/src/components/Changelog';
import Heading from '@theme/Heading';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import MailingListForm from '@site/src/components/MailingListForm';
import MDXComponents from '@theme/MDXComponents';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import {commitTypeName, sortCommitTypes} from '@site/src/exports/commits';
import dateFormat from 'dateformat';
import pluralize from 'pluralize';
import Signalz from '@site/src/exports/signalz';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewRelease} from '@site/src/exports/newRelease';

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

function Sidebar({releases, release}) {
  return (
    <div className={classnames(styles.sidebar, 'sidebar', 'sidebar--right')}>
      <div className="menu">
        <ul className="menu__list">
          <li className="menu__list-item">
            <div className="menu__list-title">Releases</div>

            <ul className="menu__list">
              {releases.map((menuRelease, idx) =>
                <li className="menu__list-item" key={idx}>
                  <Link to={`/releases/${menuRelease.version}`} className={classnames('menu__link', styles.sidebarMenuLink, {'menu__link--active': release == menuRelease})}>
                    <div>{idx == 0 && <small className="badge badge--secondary" style={{marginRight: "5px", position: "relative", top: "-2px"}}>latest</small>} v{menuRelease.version}</div>
                    <div><small>{dateFormat(Date.parse(menuRelease.date), "mmm dS, yyyy")}</small></div>
                  </Link>
                </li>
              )}
            </ul>
          </li>
        </ul>
      </div>
    </div>
  )
}

function Highlight({post}) {
  return (
    <div className="section">
      <AnchoredH3 id={post.id}>{post.title}</AnchoredH3>
      <div dangerouslySetInnerHTML={{__html: post.body}} />
    </div>
  );
}

function BlogHighlight({post}) {
  const date = Date.parse(post.date);
  const MAX_LENGTH = 175;

  return (
    <div className="section">
      <div className="badges">
        <Tags tags={post.tags} valuesOnly={true} />
      </div>
      <AnchoredH3 id={post.id}><Link to={`/blog/${post.id}`}>{post.title}</Link></AnchoredH3>
      <Avatar github={post.author_github} size="sm" subTitle={dateFormat(date, "mmmm dS, yyyy")} className="sub__title" />
      <p>
        {post.description.substring(0, MAX_LENGTH)}... <Link to={`/blog/${post.id}`}>read the full post</Link>
      </p>
    </div>
  );
}

function UpgradeGuide({upgradeGuide, key}) {
  return (
    <div className="section">
      <AnchoredH3 id={upgradeGuide.id}>{upgradeGuide.title}</AnchoredH3>
      <div dangerouslySetInnerHTML={{__html: upgradeGuide.body}} />
    </div>
  );
}

function ChangelogSentence({release}) {
  const groupedCommits = _.groupBy(release.commits, 'type');
  const groupKeys = sortCommitTypes(Object.keys(groupedCommits));
  const posts = release.posts;

  return groupKeys.filter(key => !['docs', 'chore'].includes(key)).map((groupKey, idx) => (
    <>
      {idx == (groupKeys.length - 1) ? ', and ' : (idx == 0 ? '' : ', ')}
      <a href={`#${groupKey}`} className="contents__link">{pluralize(commitTypeName(groupKey).toLowerCase(), groupedCommits[groupKey].length, true)}</a>
    </>
  ));
}

function Notes({release, latest}) {
  const subtitle = release.subtitle || (<>Released by <Link to="/community#team">Ben</Link></>);
  const description = release.description || "";
  const date = Date.parse(release.date);
  const posts = release.posts;
  const highlights = release.highlights;
  posts.reverse();

  let releaseTypeClass = 'primary';

  switch(release.type) {
    case 'initial dev':
      releaseTypeClass = 'warning';
      break;
    case 'major':
      releaseTypeClass = 'warning';
      break;
  }

  return (
    <article className={styles.content}>
      <header className={styles.header}>
        <div className="container container--fluid">
          <div className={styles.componentsHeroOverlay}>
            <h1>Vector v{release.version} Release Notes</h1>
            <div className="hero--subtitle">
              <div className={styles.heroSubTitle}>
                {subtitle}, <time>{dateFormat(date, "mmmm dS, yyyy")}</time>
              </div>
              <div>
                <small>
                  {latest ?
                    <span className="badge badge--primary badge--rounded" title="This is the latest (recommended) stable release"><i className="feather icon-check"></i> latest</span> :
                    <a href="/releases/latest" className="badge badge--warning badge--rounded" title="This release is outdated, newer releases are available"><i className="feather icon-alert-triangle"></i> outdated</a>}
                  &nbsp;&nbsp;
                  <a href={release.type_url} target="_blank" className={classnames('badge', `badge--${releaseTypeClass}`, 'badge--rounded')} title={`This is a ${release.type} release as defined by the semantic versioning spec`}><i className="feather icon-chevrons-up"></i> {release.type}</a>
                  &nbsp;&nbsp;
                  <a href={release.compare_url} target="_blank" className="badge badge--primary badge--rounded" title={`View the diff since ${release.last_version}`}>+{release.insertions_count}, -{release.deletions_count}</a>
                </small>
              </div>
            </div>
            <div className="badges">
            </div>
          </div>
        </div>
      </header>
      <section className="shade" style={{textAlign: 'center'}}>
        <MailingListForm center={true} />
      </section>
      <section className="markdown">
        {description.length > 0 && <p>{description}</p>}
        <p>
          We're excited to release Vector v{release.version}! Vector follows <a href="https://semver.org" target="_blank">semantic versioning</a>, and this is an <a href={release.type_url} target="_blank">{release.type}</a> release. This release brings <ChangelogSentence release={release} />. Checkout the <a href="#highlights">highlights</a> for notable features and, as always, <Link to="/community/">let us know what you think</Link>!
        </p>

        {posts.length > 0 || highlights.length > 0 && (
          <>
            <AnchoredH2 id="highlights">Highlights</AnchoredH2>

            <div className="section-list">
              {posts.map((post, idx) => (
                <BlogHighlight post={post} key={idx} />
              ))}
              {highlights.map((post, idx) => (
                <Highlight post={post} key={idx} />
              ))}
            </div>
          </>
        )}

        {release.upgrade_guides.length > 0 && (
          <>
            <AnchoredH2 id="breaking-changes" className="text--danger"><i className="feather icon-alert-triangle"></i> Breaking Changes</AnchoredH2>

            <div className="section-list">
              {release.upgrade_guides.map((upgradeGuide, idx) => (
                <UpgradeGuide upgradeGuide={upgradeGuide} key={idx} />
              ))}
            </div>
          </>
        )}

        <AnchoredH2 id="overview">Changelog</AnchoredH2>

        <Changelog commits={release.commits} />

        <hr />

        <Jump to={`/releases/${release.version}/download`}>Download this release</Jump>
      </section>
    </article>
  );
}

function TableOfContents({release}) {
  const groupedCommits = _.groupBy(release.commits, 'type');
  const groupKeys = sortCommitTypes(Object.keys(groupedCommits));
  const posts = release.posts;
  const highlights = release.highlights;

  return (
    <div className={styles.toc}>
      <div className="table-of-contents">
        <div className="section">
          <div className="title">Contents</div>

          <ul className="contents">
            {posts.length > 0 || highlights.length > 0 && (
              <li>
                <a href="#highlights" className="contents__link">Highlights</a>
                <ul>
                  {posts.map((post, idx) =>
                    <li key={idx}>
                      <a href={`#${post.id}`} className="contents__link" title={post.title}>{post.title}</a>
                    </li>
                  )}
                  {highlights.map((post, idx) =>
                    <li key={idx}>
                      <a href={`#${post.id}`} className="contents__link" title={post.title}>{post.title}</a>
                    </li>
                  )}
                </ul>
              </li>
            )}
            {release.upgrade_guides.length > 0 && (
              <li>
                <a href="#breaking-changes" className="contents__link text--danger"><i className="feather icon-alert-triangle"></i> Breaking Changes</a>
                <ul>
                  {release.upgrade_guides.map((upgradeGuide, idx) =>
                    <li key={idx}>
                      <a href={`#${upgradeGuide.id}`} className="contents__link" title={upgradeGuide.title}>{upgradeGuide.title}</a>
                    </li>
                  )}
                </ul>
              </li>
            )}
            <li>
              <a href="#" className="contents__link">Changelog</a>
              <ul>
                {groupKeys.map((groupKey, idx) =>
                  <li key={idx}>
                    <a href={`#${groupKey}`} className="contents__link">{pluralize(commitTypeName(groupKey), groupedCommits[groupKey].length, true)}</a>
                  </li>
                )}
              </ul>
            </li>
          </ul>
        </div>

        <div className="section">
          <div className="title">Resources</div>

          <ul className="contents">
            <li>
              <a href={release.compare_url} target="_blank" className="contents__link">
                <i className="feather icon-code"></i> Compare
              </a>
            </li>
            <li>
              <Link to={`/releases/${release.version}/download`} className="contents__link">
                <i className="feather icon-download"></i> Download
              </Link>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
}

function ReleaseNotes({version}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;
  const release = releases[version];
  const releasesList = Object.values(releases).reverse();
  const latest = releasesList[0] == release;

  if (latest) {
    viewedNewRelease();
  }

  return (
    <Layout title={`v${version} Release Notes`} description={`Vector v${version} release notes. Highlights, changes, and updates.`}>
      <main>
        <div className={styles.containers}>
          <Sidebar releases={releasesList} release={release} />
          <Notes release={release} latest={latest} />
          <TableOfContents release={release} />
        </div>
      </main>
    </Layout>
  );
}

export default ReleaseNotes;
