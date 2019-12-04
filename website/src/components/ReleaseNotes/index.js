import React, { useEffect } from 'react';

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

function getRelease(version) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;

  return releases[version];
}

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
  const release = getRelease(version);
  const date = Date.parse(release.date);
  const groupedCommits = _.groupBy(release.commits, 'group');
  const groupKeys = sortCommitTypes(Object.keys(groupedCommits));

  return (
    <div className={styles.containers}>
      <div className={styles.sidebar}>
        sidebar
      </div>
      <article className={styles.content}>
        <header className={styles.header}>
          <div className="container container--fluid">
            <div className={styles.componentsHeroOverlay}>
              <h1>Vector v{version} Release Notes</h1>
              <div className="hero__subtitle">
                Released on <time>{dateFormat(date, "mmmm dS, yyyy")}</time> by <Link to="/community#team">Ben</Link>
              </div>
            </div>
          </div>
        </header>
        <section className="markdown">
          <p>
            We're excited to release Vector v{version}! Vector follows semantic versioning, and this is an <a href="/">initial development release</a>.

             and this is a <code>minor</code> release which is backwards compatible with the 2.X line.
          </p>

          <AnchoredH2 id="highlights">Highlights</AnchoredH2>

          <Jump to="/" className="jump-to--highlight">
            <span className="badge badge--highlight badge--rounded">config</span> Unit Testing: Treating Your Vector Config Files As Code <small> - by Ashley</small>
          </Jump>

          <Jump to="/" className="jump-to--highlight">
            <span className="badge badge--highlight badge--rounded">platforms</span> Windows support is here!  <small> - by Alex</small>
          </Jump>

          <Jump to="/" className="jump-to--highlight">
            <span className="badge badge--highlight badge--rounded">platforms</span> ARMv7, ARM64, and IoT  <small> - by Alex</small>
          </Jump>

          <AnchoredH2 id="overview">Changelog</AnchoredH2>

          <Changelog commits={release.commits} />
        </section>
      </article>
      <div className={styles.toc}>
        <div className="table-of-contents">
          <div className="section">
            <div className="title">Stats</div>

            <ul className="contents">
              <li>
                <a href={release.compare_url} target="_blank" className="contents__link">
                  <i className="feather icon-code"></i> +{release.insertions_count}, -{release.deletions_count}
                </a>
              </li>
              <li>
                <a href="/docs/about/guarantees#prod-ready" className="contents__link">
                  <i className="feather icon-users"></i> 10 contributors
                </a>
              </li>
            </ul>
          </div>
          <div className="section">
            <div className="title">Contents</div>

            <ul className="contents">
              <li>
                <a href="#" className="contents__link">Highlights</a>
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
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ReleaseNotes;