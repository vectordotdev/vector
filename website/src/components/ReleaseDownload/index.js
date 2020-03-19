import React, {useState} from 'react';

import Alert from '@site/src/components/Alert';
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
  const {containers, package_managers: packageManagers, operating_systems: operatingSystems} = installation;

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
        <div>Containers</div>
        <div>
          {Object.values(containers).map((container, idx) => (
            <span key={idx}>
              {idx > 0 ? " • " : ""}
              <Link to={`/docs/setup/installation/containers/${container.id}/`}> {container.name}</Link>
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
              <Link to={`/docs/setup/installation/package-managers/${packageManager.id}/`}>{packageManager.name}</Link>
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
              <Link to={`/docs/setup/installation/operating-systems/${operatingSystem.id}/`}>{operatingSystem.name}</Link>
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
  const {metadata: {installation: installation, latest_release: latestRelease, releases}} = siteConfig.customFields;
  const {downloads} = installation;

  const latestDownloads = Object.values(downloads).filter(download => download.available_on_latest);
  const nightlyDownloads = Object.values(downloads).filter(download => download.available_on_nightly);
  const nightlyDate = new Date().toISOString().substr(0,10);

  const oldReleases = Object.values(releases).slice(0);
  oldReleases.pop();
  oldReleases.reverse();
  const olderOptions = oldReleases.map(release => ({value: release.version, label: `v${release.version} - ${release.date}`}));

  viewedNewRelease();

  const [selectedVersion, setVersion] = useState(version || latestRelease.version);
  const selectedTab = selectedVersion == latestRelease.version ? 'latest' : 'older';
  const oldRelease = selectedVersion == latestRelease.version ? oldReleases[0] : oldReleases.find(release => release.version == selectedVersion);

  return (
    <Layout title="Download" description="Download Vector for your platform.">
      <header className={classnames('hero', styles.downloadHeroBanner)}>
        <div className="container">
          <div className={styles.downloadLine}>
            <div></div>
          </div>
          <svg width="104px" height="77px" viewBox="0 0 104 77" version="1.1" xmlns="http://www.w3.org/2000/svg">
              <g id="Download" stroke="none" strokeWidth="1" fill="none" fillRule="evenodd">
                  <g id="Custom-Preset" transform="translate(-514.000000, -182.000000)">
                      <g id="Download" transform="translate(-340.000000, -479.000000)">
                          <g id="Box" transform="translate(855.000000, 662.000000)">
                              <polygon id="Stroke-1" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="0 53.8054695 50.8700833 75 101.73913 53.8054695 50.8700833 32.6086957"></polygon>
                              <polygon id="Stroke-7" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="50.2173913 1.30434783 101.73913 22.3096677 101.48902 53.4782609 50.3236883 32.7001761"></polygon>
                              <polygon id="Stroke-9" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="0.652173913 22.0405003 0.652173913 53.4782609 51.5217391 32.6717368 51.415679 1.30434783"></polygon>
                              <polygon id="Fill-11" fillOpacity="0.061489292" fill="#000000" points="0.105238838 53.6435064 0 21.3702827 50.7622629 0 101.73913 21.5851512 101.571821 53.5681562 50.7622629 75"></polygon>
                              <polygon id="Stroke-13" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="0.105238838 53.6435107 0 21.3702935 50.7622629 0 101.73913 21.5851619 101.57178 53.5681605 50.7622629 75"></polygon>
                              <g id="Vector" transform="translate(17.608696, 16.304348)">
                                  <polygon id="Fill-21" fill="#10E7FF" points="0 32.7525563 32.9745185 45.7396607 65.9482759 32.7525563 32.9745185 19.7640509"></polygon>
                                  <polygon id="Fill-22" fill="#10E7FF" points="0 12.9878049 0 32.705776 33.8343328 45.7396607 33.8343328 26.1899524"></polygon>
                                  <polygon id="Fill-23" fill="#10E7FF" points="33.8343328 26.0884369 66.5217391 12.9878049 66.3608828 32.6389942 33.8343328 45.7396607"></polygon>
                                  <polygon id="Fill-24" fill="#10E7FF" points="33.2608696 0 66.5217391 13.1859105 66.3602357 32.7518558 33.3294971 19.7085585"></polygon>
                                  <polygon id="Fill-25" fill="#10E7FF" points="0 13.0169892 0 32.7518558 33.2608696 19.690725 33.1920696 0"></polygon>
                                  <polygon id="Fill-26" fill="#10E7FF" points="0.0688155922 32.7151525 0 13.0329132 33.1907541 0 66.5217391 13.1640006 66.4123223 32.6691948 33.1907541 45.7396607"></polygon>
                                  <polygon id="Fill-21-Copy" fillOpacity="0.30736451" fill="#FFFFFF" points="0 12.9885054 32.9745185 25.9756098 65.9482759 12.9885054 32.9745185 0"></polygon>
                              </g>
                              <polygon id="Stroke-3" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="0.652173913 22.826087 0.652173913 54.2369271 51.5217391 75 51.5217391 43.8572217"></polygon>
                              <polygon id="Stroke-5" strokeOpacity="0.0961538462" stroke="#000000" strokeLinecap="round" strokeLinejoin="round" points="50.2173913 43.6955049 101.73913 22.826087 101.48811 54.130582 50.2173913 75"></polygon>
                          </g>
                      </g>
                  </g>
              </g>
          </svg>
          <h1>Download Vector</h1>
        </div>
      </header>
      <main>
        <section>
          <div className={classnames('container', styles.downloadTableContainer)}>
            <Tabs
              block={true}
              className="rounded"
              defaultValue={selectedTab}
              values={[
                { label: 'Older', value: 'older', },
                { label: `Latest (${latestRelease.version})`, value: 'latest', },
                { label: 'Nightly', value: 'nightly', },
              ]
            }>
            <TabItem value="older">
              <Alert fill={true} type="warning">
                Olders versions are outdated and it is highly recommended to use the latest version. Please proceed with caution.
              </Alert>
              <Select
                className={classnames('react-select-container', styles.releaseSelect)}
                classNamePrefix="react-select"
                options={olderOptions}
                isClearable={false}
                placeholder="Select a version..."
                value={olderOptions.find(option => option.value == oldRelease.version)}
                onChange={(selectedOption) => setVersion(selectedOption ? selectedOption.value : null)} />
              <DownloadTable browsePath={oldRelease.version} date={oldRelease.date} downloads={latestDownloads} releaseNotesPath={`/releases/${oldRelease.version}/`} version={oldRelease.version} />
            </TabItem>
            <TabItem value="latest">
              <DownloadTable browsePath={latestRelease.version} date={latestRelease.date} downloads={latestDownloads} releaseNotesPath={`/releases/${latestRelease.version}/`} version={latestRelease.version} />
            </TabItem>
            <TabItem value="nightly">
              <Alert fill={true} type="warning">
                Nightly versions contain bleeding edge changes that may contain bugs. Proceed with caution.
              </Alert>
              <DownloadTable browsePath="nightly/latest" date={nightlyDate} downloads={nightlyDownloads} version="nightly" />
            </TabItem>
            </Tabs>
          </div>
        </section>
        <section>
          <div className={classnames('container', styles.downloadGetStartedContainer)}>
            <h2>Ready to get started?</h2>
            <Jump to="/docs/setup/guides/getting-started/">
              <i className="feather icon-book-open"></i> Follow the getting started guide
            </Jump>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default ReleaseDownload;
