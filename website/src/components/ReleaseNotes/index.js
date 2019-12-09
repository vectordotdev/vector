import React, { useEffect } from 'react';

import Alert from '@site/src/components/Alert';
import Changelog from '@site/src/components/Changelog';
import Heading from '@theme/Heading';
import Jump from '@site/src/components/Jump';
import Link from '@docusaurus/Link';

import classnames from 'classnames';
import {commitTypeName, sortCommitTypes} from '@site/src/exports/commits';
import dateFormat from 'dateformat';
import pluralize from 'pluralize';
import Signalz from '@site/src/exports/signalz';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const AnchoredH2 = Heading('h2');

function ReleaseSummary({release}) {
  if (semver.lt(version, '1')) {
    return(
      <p>
        We're excited to release Vector v{version}! Vector follows semantic versioning, and this is an <a href="/">initial development release</a>.
      </p>
    );
  } else {

  }
}

function ReleaseNotes({version}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;
  const release = releases[version];
  const date = Date.parse(release.date);
  const groupedCommits = _.groupBy(release.commits, 'type');
  const groupKeys = sortCommitTypes(Object.keys(groupedCommits));
  const releasesList = Object.values(releases).reverse();
  const latest = releasesList[0] == release;

  return (
    <div className={styles.containers}>
      <div className={classnames(styles.sidebar, 'sidebar', 'sidebar--right')}>
        <div className="menu">
          <ul className="menu__list">
            <li className="menu__list-item">
              <div className="menu__list-title">Releases</div>

              <ul className="menu__list">
                {releasesList.map((menuRelease, idx) =>
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
      <article className={styles.content}>
        <header className={styles.header}>
          <div className="container container--fluid">
            <div className={styles.componentsHeroOverlay}>
              <h1>Vector v{version} Release Notes</h1>
              <div className="hero__subtitle">
                <div>Released on <time>{dateFormat(date, "mmmm dS, yyyy")}</time> by <Link to="/community#team">Ben</Link></div>
                <div>
                  <small>
                    {latest ?
                      <span className="badge badge--primary badge--rounded" title="This is the latest stable release"><i className="feather icon-check"></i> latest</span> :
                      <a href="/releases/latest" className="badge badge--warning badge--rounded" title="This release is outdated, newer releases are available"><i className="feather icon-alert-triangle"></i> outdated</a>}
                    &nbsp;&nbsp;
                    <a href={release.type_url} target="_blank" class="badge badge--primary badge--rounded" title="Semantic increment type"><i className="feather icon-chevrons-up"></i> {release.type}</a>
                    &nbsp;&nbsp;
                    <a href={release.compare_url} target="_blank" className="badge badge--primary badge--rounded" title="diff/compare">+{release.insertions_count}, -{release.deletions_count}</a>
                  </small>
                </div>
              </div>
              <div className="badges">
              </div>
            </div>
          </div>
        </header>
        <section className="markdown">
          <p>
            We're excited to release Vector v{version}! Vector follows <a href="https://semver.org" target="_blank">semantic versioning</a>, and this is an <a href={release.type_url} target="_blank">{release.type}</a> release.
          </p>

          <AnchoredH2 id="highlights">Highlights</AnchoredH2>

          <Jump to="/" className="jump-to--highlight">
            config | Unit Testing: Treating Your Vector Config Files As Code <small> - by Ashley</small>
          </Jump>

          <Jump to="/" className="jump-to--highlight">
            <span className="badge badge--highlight">platforms</span> Windows support is here!  <small> - by Alex</small>
          </Jump>

          <Jump to="/" className="jump-to--highlight">
            <span className="badge badge--highlight badge--category">platforms</span> ARMv7, ARM64, and IoT  <small> - by Alex</small>
          </Jump>

          <AnchoredH2 id="highlights">Breaking Changes</AnchoredH2>

          <Alert icon="thumbs-up" type="primary">This release contains no breaking changes.</Alert>

          <AnchoredH2 id="overview">Changelog</AnchoredH2>

          <Changelog commits={release.commits} />

          <AnchoredH2 id="overview">Roadmap</AnchoredH2>

          <hr />

          <Jump to={`/releases/${release.version}/download`}>Download this release</Jump>
        </section>
      </article>
      <div className={styles.toc}>
        <div className="table-of-contents">
          <div className="section">
            <div className="title">Contents</div>

            <ul className="contents">
              <li>
                <a href="#highlights" className="contents__link">2 Highlights</a>
              </li>
              <li>
                <a href="#breaking-changes" className="contents__link">3 Breaking Changes</a>
              </li>
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
              <li>
                <a href="#breaking-changes" className="contents__link">Roadmap</a>
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
    </div>
  );
}

export default ReleaseNotes;