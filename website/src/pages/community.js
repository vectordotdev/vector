import React from 'react';

import Layout from '@theme/Layout';

import styles from './community.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Community() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;

  return (
    <Layout title="Community">
      <header className="hero">
        <div className="container container--fluid">
          <h1>Vector Community</h1>
          <div className="mailing-list">
            <div className="mailing-list--description">
              The easiest way to stay up-to-date. One email per month. No spam, ever.
            </div>
            <form action="https://app.getvero.com/forms/a748ded7ce0da69e6042fa1e21042506" method="post">
              <div class="subscribe_form">
                <input className="input input--lg" name="email" placeholder="you@email.com" type="email" />
                <button className="button button--primary button--lg" type="submit">Subscribe</button>
              </div>
            </form>
          </div>
        </div>
      </header>
      <main>
        <section>
          <div className="container">
            <h2>Connect</h2>
            <div className="row">
              <div className="col">
                <a href="https://chat.vector.dev" target="_blank" className="panel panel--link text--center">
                  <div className="panel--icon">
                    <i className="feather icon-message-circle"></i>
                  </div>
                  <div className="panel--title">Chat</div>
                  <div className="panel--description">Ask questions and get help</div>
                </a>
              </div>
              <div className="col">
                <a href="https://twitter.com/timberdotio" target="_blank" className="panel panel--link text--center">
                  <div className="panel--icon">
                    <i className="feather icon-twitter"></i>
                  </div>
                  <div className="panel--title">Twitter @timberdotio</div>
                  <div className="panel--description">
                    Stay up-to-date with <code>#vector</code></div>
                </a>
              </div>
              <div className="col">
                <a href="https://github.com/timberio/vector" target="_blank" className="panel panel--link text--center">
                  <div className="panel--icon">
                    <i className="feather icon-github"></i>
                  </div>
                  <div className="panel--title">Github timberio/vector</div>
                  <div className="panel--description">Issues, code, and development</div>
                </a>
              </div>
            </div>
          </div>
        </section>
        <section>
          <div className="container">
            <h2>Contribute</h2>
            
            <p>
              Vector is <a href="https://github.com/timberio/vector">open-source</a> and welcomes contributions. A few guidelines to help you get started:
            </p>
            <ol>
              <li>Read our <a href="https://github.com/timberio/vector/blob/master/CONTRIBUTING.md">contribution guide</a>.</li>
              <li>Start with <a href="https://github.com/timberio/vector/contribute">good first issues</a>.</li>
              <li>Join our <a href="https://chat.vector.dev">chat</a> if you have any questions.</li>
            </ol>
          </div>
        </section>
        <section>
          <div className="container">
            <h2>Meet The Core Team</h2>
            <div className={styles.coreTeam}>
               {team.map((member, idx) => (
                  <div key={idx} className="avatar avatar--vertical">
                    <img
                      className="avatar__photo avatar__photo--xl"
                      src={member.avatar}
                    />
                    <div className="avatar__intro">
                      <h4 className="avatar__name">{member.name}</h4>
                    </div>
                  </div>
                ))}
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Community;