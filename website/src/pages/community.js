import React from 'react';

import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import styles from './community.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

function Community() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;

  return (
    <Layout title="Community" description="Join the Vector community. Connect with other Vector users and help make Vector better.">
      <header className="hero hero--clean">
        <div className="container container--fluid">
          <h1>Vector Community</h1>
          <div className="hero--subtitle">Join the Vector community. Connect with other Vector users and help make Vector better.</div>
        </div>
      </header>
      <main>
        <section>
          <div className="container">
            <div className="row">
              <div className="col">
                <a href="https://chat.vector.dev" target="_blank" className="panel panel--button">
                  <div className="panel--icon">
                    <i className="feather icon-message-circle"></i>
                  </div>
                  <div className="panel--title">Chat</div>
                  <div className="panel--description">Ask questions and get help</div>
                </a>
              </div>
              <div className="col">
                <a href="https://twitter.com/vectordotdev" target="_blank" className="panel panel--button">
                  <div className="panel--icon">
                    <i className="feather icon-twitter" title="Twitter"></i>
                  </div>
                  <div className="panel--title">@vectordotdev</div>
                  <div className="panel--description">Follow us in real-time</div>
                </a>
              </div>
              <div className="col">
                <a href="https://github.com/timberio/vector" target="_blank" className="panel panel--button">
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
            <AnchoredH2 id="team">Meet The Team</AnchoredH2>
            <div className="sub-title">A simple composable format lets you build flexible pipelines</div>

            <div className={styles.coreTeam}>
              {team.map((member, idx) => (
                <Link key={idx} to={member.github} className="avatar avatar--vertical">
                  <img
                    className="avatar__photo avatar__photo--xl"
                    src={member.avatar}
                  />
                  <div className="avatar__intro">
                    <h4 className="avatar__name">{member.name}</h4>
                  </div>
                </Link>
              ))}
            </div>
          </div>
        </section>
        <section>
          <div className="container">
            <AnchoredH2 id="faqs">FAQs</AnchoredH2>

            <AnchoredH3 id="contribute" className="header--flush">How do I contribute to Vector?</AnchoredH3>

            <p>
              Vector is <a href="https://github.com/timberio/vector">open-source</a> and welcomes contributions. A few guidelines to help you get started:
            </p>
            <ol>
              <li>Read our <a href="https://github.com/timberio/vector/blob/master/CONTRIBUTING.md">contribution guide</a>.</li>
              <li>Start with <a href="https://github.com/timberio/vector/contribute">good first issues</a>.</li>
              <li>Join our <a href="https://chat.vector.dev">chat</a> if you have any questions. We are happy to help!</li>
            </ol>

            <AnchoredH3 id="contribute" className="header--flush margin-top--lg">What is the Vector governance model?</AnchoredH3>

            <p>
              Vector's high-level governance model is designed around the requirements and best practices of the CNCF / Linux Foundation Core Infrastructure Initiative best practice targeting a silver badge status.
            </p>
            <ol>
              <li><a href="https://bestpractices.coreinfrastructure.org/en" target="_blank">CNCF CII best practices</a></li>
              <li><a href="https://www.linuxfoundation.org/" target="_blank">Linux Foundation</a></li>
            </ol>

            <AnchoredH3 id="contribute" className="header--flush margin-top--lg">What is the Vector project model?</AnchoredH3>

            <p>
              Vector's project / product management model is designed around the linux kernel development practices and processes.
            </p>
            <ol>
              <li><a href="https://bestpractices.coreinfrastructure.org/en/projects/34" target="_blank">Linux Kernel CII best practices</a></li>
              <li><a href="https://www.kernel.org/" target="_blank">Linux Foundation</a></li>
            </ol>

            <AnchoredH3 id="contribute" className="header--flush margin-top--lg">What is the Vector community model?</AnchoredH3>

            <p>
              Vector has adopted the Rust community model and practices for engaging with people and ensuring that all contributors and stakeholders respect the code of conduct.
            </p>
            <ol>
              <li><a href="https://www.rust-lang.org/" target="_blank">Rust Language Organization</a></li>
              <li><a href="https://www.rust-lang.org/community" target="_blank">Rust community standards</a></li>
            </ol>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Community;
