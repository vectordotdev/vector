import React from 'react';

import Layout from '@theme/Layout';

import styles from './community.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Contact() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;

  return (
    <Layout title="Contact" description="Contact the Vector and Timber.io team">
      <header className="hero">
        <div className="container container--fluid">
          <h1>Contact</h1>
          <div className="hero--subtitle">
            Vector is a <a href="https://timber.io">Timber.io</a> open-source product. You can contact the Vector &amp; Timber team using any of the options below.
          </div>
        </div>
      </header>
      <main>
        <section>
          <div className="container">
            <div className="row">
              <div className="col">
                <a href="mailto:hi@timber.io" className="panel text--center">
                  <div className="panel--icon">
                    <i className="feather icon-mail"></i>
                  </div>
                  <div className="panel--title">hi@timber.io</div>
                  <div className="panel--description">Shoot us an email</div>
                </a>
              </div>
              <div className="col">
                <a href="https://twitter.com/vectordotdev" target="_blank" className="panel text--center">
                  <div className="panel--icon">
                    <i className="feather icon-twitter"></i>
                  </div>
                  <div className="panel--title">@vectordotdev</div>
                  <div className="panel--description">
                    Tweet at us
                  </div>
                </a>
              </div>
              <div className="col">
                <a href="https://chat.vector.dev" target="_blank" className="panel text--center">
                  <div className="panel--icon">
                    <i className="feather icon-message-circle"></i>
                  </div>
                  <div className="panel--title">Chat</div>
                  <div className="panel--description">Join our chat</div>
                </a>
              </div>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Contact;
