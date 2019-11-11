import React from 'react';

import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import SVG from 'react-inlinesvg';
import TabItem from '@theme/TabItem'
import Tabs from '@theme/Tabs'

import classnames from 'classnames';
import styles from './download.module.css';

function Download() {
  return (
    <Layout title="Download Vector">
      <header className={classnames('hero', styles.downloadHeroBanner)}>
        <div className="container">
          <SVG src="/img/download.svg" />
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
                { label: 'Latest', value: 'latest', },
                { label: 'Nightly', value: 'nightly', },
              ]
            }>
            <TabItem value="latest">
              <table className={styles.downloadTable}>
                <tr>
                  <td>Version</td>
                  <td>
                    0.5.0 - Jan 2, 2018 - <a href="#">release notes</a>
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
                      <a href="#"><i className="feather icon-download"></i> vector-amd64.deb</a>
                    </div>
                    <div>
                      <a href="#"><i className="feather icon-download"></i> vector-x86_64-apple-darwin.tar.gz</a>
                    </div>
                    <div>
                      <a href="https://packages.timber.io/vector/" target="_blank">browse all files&hellip;</a>
                    </div>
                  </td>
                </tr>
                <tr>
                  <td>Package Managers</td>
                  <td>
                    <a href="#">DPKG</a>
                  </td>
                </tr>
                <tr>
                  <td>Platforms</td>
                  <td>
                    <a href="#">Docker</a>
                  </td>
                </tr>
              </table>
            </TabItem>
            <TabItem value="nightly">
              fdsf
            </TabItem>
            </Tabs>
          </div>
        </section>
        <section>
          <div className={classnames('container', styles.downloadGetStartedContainer)}>
            <h2>Ready to get started?</h2>
            <Jump to="/">
              <i className="feather icon-book-open"></i> Follow the getting started guide
            </Jump>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Download;