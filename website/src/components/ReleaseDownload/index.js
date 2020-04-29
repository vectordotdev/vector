import React, {useState} from 'react';

import Alert from '@site/src/components/Alert';
import DownloadDiagram from '@site/src/components/DownloadDiagram';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import Select from 'react-select';
import SVG from 'react-inlinesvg';
import TabItem from '@theme/TabItem'
import Tabs from '@theme/Tabs'

import classnames from 'classnames';
import groupBy from 'lodash/groupBy';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewRelease} from '@site/src/exports/newRelease';

function Downloads({browsePath, downloads}) {
  const groupedDownloads = groupBy(downloads, ((download) => {
    return [download.os, download.package_manager];
  }));

  return (
    <ul className={styles.downloadFiles}>
      {Object.keys(groupedDownloads).sort().map((key, catIdx) => (
        <li key={catIdx}>
          <span>{groupedDownloads[key][0].os} <code>.{groupedDownloads[key][0].file_type}</code></span>
          <ul>
            {groupedDownloads[key].map((download, downloadIdx) => (
              <li key={downloadIdx}>
                <a key={downloadIdx} title={download.file_name} href={`https://packages.timber.io/vector/${browsePath}/${download.file_name}`}>
                  <i className="feather icon-download"></i> {download.arch}
                </a>
              </li>
            ))}
          </ul>
        </li>
      ))}
      <li>
        <a href={`https://packages.timber.io/vector/${browsePath}/`} target="_blank">browse all files&hellip;</a>
      </li>
    </ul>
  );
}

function DownloadTable({browsePath, date, downloads, releaseNotesPath, version}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {installation: installation, latest_release: latestRelease}} = siteConfig.customFields;
  const {package_managers: packageManagers, platforms, operating_systems: operatingSystems} = installation;

  return (
    <div className={styles.downloadTable}>
      <div>
        <div>Version</div>
        <div>
          {version} • {date}{releaseNotesPath && <> • <Link to={releaseNotesPath}>release notes</Link></>}
        </div>
      </div>
      <div>
        <div>License</div>
        <div>
          <a href="https://github.com/timberio/vector/blob/master/LICENSE" target="_blank">Apache 2</a>
        </div>
      </div>
      <div>
        <div>Downloads</div>
        <div>
          <Downloads downloads={downloads} browsePath={browsePath} />
        </div>
      </div>
      <div>
        <div>Platforms</div>
        <div>
          {Object.values(platforms).map((platform, idx) => (
            <span key={idx}>
              {idx > 0 ? " • " : ""}
              <Link to={`/docs/setup/installation/platforms/${platform.name}/`}> {platform.title}</Link>
            </span>
          ))}
        </div>
      </div>
      <div>
        <div>Package Managers</div>
        <div>
          {Object.values(packageManagers).map((packageManager, idx) => (
            <span key={idx}>
              {idx > 0 ? " • " : ""}
              <Link to={`/docs/setup/installation/package-managers/${packageManager.name}/`}>{packageManager.title}</Link>
            </span>
          ))}
        </div>
      </div>
      <div>
        <div>Operating Systems</div>
        <div>
          {Object.values(operatingSystems).map((operatingSystem, idx) => (
            <span key={idx}>
              {idx > 0 ? " • " : ""}
              <Link to={`/docs/setup/installation/operating-systems/${operatingSystem.name}/`}>{operatingSystem.title}</Link>
            </span>
          ))}
        </div>
      </div>
      <div>
        <div>Manual</div>
        <div>
          <Link to="/docs/setup/installation/manual/from-archives/">From archives</Link>
          &nbsp;•&nbsp;
          <Link to="/docs/setup/installation/manual/from-source/">From source</Link>
        </div>
      </div>
    </div>
  );
}

function ReleaseDownload({version}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {installation: installation, latest_release: latestRelease, releases: releasesObj}} = siteConfig.customFields;
  const {downloads} = installation;

  const latestDownloads = Object.values(downloads).filter(download => download.available_on_latest);
  const nightlyDownloads = Object.values(downloads).filter(download => download.available_on_nightly);
  const nightlyDate = new Date().toISOString().substr(0,10);

  const releases = Object.values(releasesObj).slice(0);
  releases.reverse();
  const releaseOptions = releases.map(release => ({value: release.version, label: `v${release.version} - ${release.date}`}));

  viewedNewRelease();

  const [selectedVersion, setVersion] = useState(version || latestRelease.version);
  const release = releases.find(release => release.version == selectedVersion);

  return (
    <Layout title="Download" description="Download Vector for your platform.">
      <header className="hero hero--clean hero--flush">
        <div className="container">
          <DownloadDiagram />
          <h1>Download Vector</h1>
        </div>
      </header>
      <main>
        <section>
          <div className={classnames('container', styles.downloadTableContainer)}>
            <Tabs
              block={true}
              className="rounded"
              defaultValue={version == 'nightly' ? 'nightly' : 'stable'}
              values={[
                { label: `Stable`, value: 'stable', },
                { label: 'Nightly', value: 'nightly', },
              ]
            }>
            <TabItem value="stable">
              <Select
                className={classnames('react-select-container', styles.releaseSelect)}
                classNamePrefix="react-select"
                options={releaseOptions}
                isClearable={false}
                placeholder="Select a version..."
                value={releaseOptions.find(option => release && option.value == release.version)}
                onChange={(selectedOption) => setVersion(selectedOption ? selectedOption.value : null)} />
              {release && release.version != releases[0].version && <Alert fill={true} type="warning">
                This is an outdated version. Outdated versions maybe contain bugs. It is recommended to use the latest version. Please proceed with caution.
              </Alert>}
              {release && <DownloadTable
                browsePath={release.version}
                date={release.date}
                downloads={latestDownloads}
                releaseNotesPath={`/releases/${release.version}/`}
                version={release.version} />}
            </TabItem>
            <TabItem value="nightly">
              <Alert fill={true} type="warning">
                Nightly versions contain bleeding edge changes that may contain bugs. Proceed with caution.
              </Alert>
              <DownloadTable
                browsePath="nightly/latest"
                date={nightlyDate}
                downloads={nightlyDownloads}
                version="nightly" />
            </TabItem>
            </Tabs>
          </div>
        </section>
        <section>
          <div className={classnames('container', styles.downloadGetStartedContainer)}>
            <h2>Ready to get started?</h2>
            <Jump to="/guides/getting-started/">
              <i className="feather icon-book-open"></i> Follow the getting started guide
            </Jump>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default ReleaseDownload;
