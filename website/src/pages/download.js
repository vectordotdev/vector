import React from 'react';

import Alert from '@site/src/components/Alert';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import SVG from 'react-inlinesvg';
import TabItem from '@theme/TabItem'
import Tabs from '@theme/Tabs'

import classnames from 'classnames';
import styles from './download.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {viewedNewRelease} from '@site/src/exports/newRelease';

function Download() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {installation: installation, latest_release: latestRelease}} = siteConfig.customFields;
  const {containers, package_managers: packageManagers, operating_systems: operatingSystems} = installation;

  viewedNewRelease();

  return (
    <Layout title="Download Vector">
      <header className={classnames('hero', styles.downloadHeroBanner)}>
        <div className="container">
          <div className={styles.downloadLine}>
            <div></div>
          </div>
          <SVG src="/img/vector-box.svg" />
          <h1>Download Vector</h1>
        </div>
      </header>
      <main>
        <section>
          <div className={classnames('container', styles.downloadTableContainer)}>
            <Tabs
              block={true}
              defaultValue="latest"
              values={[
                { label: `Latest (${latestRelease.version})`, value: 'latest', },
                { label: 'Nightly', value: 'nightly', },
              ]
            }>
            <TabItem value="latest">
              <table className={styles.downloadTable}>
                <tbody>
                  <tr>
                    <td>Version</td>
                    <td>
                      {latestRelease.version} • {latestRelease.date} • <a href={`https://github.com/timberio/vector/releases/tag/v${latestRelease.version}`} target="_blank">release notes</a>
                    </td>
                  </tr>
                  <tr>
                    <td>License</td>
                    <td>
                      <a href="https://github.com/timberio/vector/blob/master/LICENSE" target="_blank">Apache 2</a>
                    </td>
                  </tr>
                  <tr>
                    <td>Downloads</td>
                    <td>
                      {Object.keys(latestRelease.downloads).map((name, idx) => {
                        let url = latestRelease.downloads[name];

                        return (<div key={idx}>
                          <a href={url}><i className="feather icon-download"></i> {name}</a>
                        </div>);
                      })}
                      <div>
                        <a href={`https://packages.timber.io/vector/${latestRelease.version}`} target="_blank">browse all files&hellip;</a>
                      </div>
                    </td>
                  </tr>
                  <tr>
                    <td>Containers</td>
                    <td>
                      {containers.map((container, idx) => (
                        <span key={idx}>
                          {idx > 0 ? " • " : ""}
                          <Link to={`/docs/setup/installation/containers/${container.id}`}> {container.name}</Link>
                        </span>
                      ))}
                    </td>
                  </tr>
                  <tr>
                    <td>Package Managers</td>
                    <td>
                      {packageManagers.map((packageManager, idx) => (
                        <span key={idx}>
                          {idx > 0 ? " • " : ""}
                          <Link to={`/docs/setup/installation/package-managers/${packageManager.id}`}>{packageManager.name}</Link>
                        </span>
                      ))}
                    </td>
                  </tr>
                  <tr>
                    <td>Operating Systems</td>
                    <td>
                      {operatingSystems.map((operatingSystem, idx) => (
                        <span key={idx}>
                          {idx > 0 ? " • " : ""}
                          <Link to={`/docs/setup/installation/operating-systems/${operatingSystem.id}`}>{operatingSystem.name}</Link>
                        </span>
                      ))}
                    </td>
                  </tr>
                  <tr>
                    <td>Manual</td>
                    <td>
                      <Link to="/docs/setup/installation/manual/from-archives">From archives</Link> •
                      <Link to="/docs/setup/installation/manual/from-source">From source</Link>
                    </td>
                  </tr>
                </tbody>
              </table>
            </TabItem>
            <TabItem value="nightly">
              <table className={styles.downloadTable}>
                <tbody>
                  <tr>
                    <td>Version</td>
                    <td>
                      Nightly • <a href="https://github.com/timberio/vector/compare/v{latestRelease.version}...master" target="_blank">unrelease changes</a>
                    </td>
                  </tr>
                  <tr>
                    <td>License</td>
                    <td>
                      <a href="https://github.com/timberio/vector/blob/master/LICENSE" target="_blank">Apache 2</a>
                    </td>
                  </tr>
                  <tr>
                    <td>Downloads</td>
                    <td>
                      <div>
                        <Link to="https://packages.timber.io/vector/nightly/latest/vector-amd64.deb"><i className="feather icon-download"></i> vector-amd64.deb</Link>
                      </div>
                      <div>
                        <Link to="https://packages.timber.io/vector/nightly/latest/vector-x86_64-apple-darwin.tar.gz"><i className="feather icon-download"></i> vector-x86_64-apple-darwin.tar.gz</Link>
                      </div>
                      <div>
                        <Link to="https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz"><i className="feather icon-download"></i> vector-x86_64-unknown-linux-musl.tar.gz</Link>
                      </div>
                      <div>
                        <Link to="https://packages.timber.io/vector/nightly/latest/vector-x86_64.rpm"><i className="feather icon-download"></i> vector-x86_64.rpm</Link>
                      </div>
                      <div>
                        <a href={`https://packages.timber.io/vector/nightly/latest`} target="_blank">browse all files&hellip;</a>
                      </div>
                    </td>
                  </tr>
                  <tr>
                    <td>Containers</td>
                    <td>
                      <Link to="/docs/setup/installation/containers/docker#nightlies">Docker</Link>
                    </td>
                  </tr>
                  <tr>
                    <td>Manual</td>
                    <td>
                      <Link to="/docs/setup/installation/manual/from-archives">From archives</Link> •
                      <Link to="/docs/setup/installation/manual/from-source">From source</Link>
                    </td>
                  </tr>
                </tbody>
              </table>

              <Alert type="warning">
                Nightly versions contain bleeding edge changes that may contain bugs. Proceed with caution.
              </Alert>
            </TabItem>
            </Tabs>
          </div>
        </section>
        <section>
          <div className={classnames('container', styles.downloadGetStartedContainer)}>
            <h2>Ready to get started?</h2>
            <Jump to="/docs/setup/guides/getting-started">
              <i className="feather icon-book-open"></i> Follow the getting started guide
            </Jump>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Download;