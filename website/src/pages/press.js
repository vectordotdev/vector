import React from 'react';

import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import SVG from 'react-inlinesvg';

import styles from './community.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

function Contact() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;

  return (
    <Layout title="Press" description="Offical Vector resources for digital and printed materials">
      <header className="hero domain-bg domain-bg--vector">
        <div className="container container--fluid">
          <h1>Logos & Press Kit</h1>
          <div className="hero--subtitle">
            Offical Vector resources for digital and printed materials.
          </div>
        </div>
      </header>
      <main className="container container--xs">
        <section>
          <AnchoredH2 id="connect">Text</AnchoredH2>

          <div class="markdown">
            <AnchoredH3 id="connect">Description</AnchoredH3>

            <p>
              Vector is a lightweight and ultra-fast tool for building observability pipelines.
            </p>

            <AnchoredH3 id="connect">Bio</AnchoredH3>

            <p>
              Created by <a href="https://timber.io">Timber.io</a>, Vector was built with the vision of deploying a single tool to collect, transform, and route <em>all</em> observability data, without sacrificing performance or flexibility.
            </p>
            <p>
              Since its initial release in July of 2019 Vector has grown to over 100,000 downloads per day, empowering Fortune 500 companies and startups alike to leverage the latest and greatest observability tools and methodologies.
            </p>
            <p>
              Vector's capabilities continue to expand at a rapid pace, aspiring to integrate with any and all observability products in a way that's simple, predictable and stable. No compromises.
            </p>
          </div>
        </section>
        <section>
          <AnchoredH2 id="connect">Logo</AnchoredH2>

          <AnchoredH3 id="connect">Vertical</AnchoredH3>

          <div className="row margin-bottom--lg">
            <div className="col text--center">
              <div style={{background: 'white'}} className="margin-bottom--sm">
                <a href="/press/vector-logo-vertical-light.svg">
                  <img src="/press/vector-logo-vertical-light.svg" width="100%" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-logo-vertical-light.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-logo-vertical-light.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
            <div className="col text--center">
              <div style={{background: 'black'}} className="margin-bottom--sm">
                <a href="/press/vector-logo-vertical-dark.svg">
                  <img src="/press/vector-logo-vertical-dark.svg" width="100%" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-logo-vertical-dark.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-logo-vertical-dark.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
          </div>

          <AnchoredH3 id="connect">Horizontal</AnchoredH3>

          <div className="row margin-bottom--lg">
            <div className="col text--center">
              <div style={{background: 'white'}} className="margin-bottom--sm">
                <a href="/press/vector-logo-horizontal-light.svg">
                  <img src="/press/vector-logo-horizontal-light.svg" width="100%" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-logo-horizontal-light.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-logo-horizontal-light.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
            <div className="col text--center">
              <div style={{background: 'black'}} className="margin-bottom--sm">
                <a href="/press/vector-logo-horizontal-dark.svg">
                  <img src="/press/vector-logo-horizontal-dark.svg" width="100%" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-logo-horizontal-dark.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-logo-horizontal-dark.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
          </div>

          <AnchoredH3 id="connect">Icon</AnchoredH3>

          <div className="row margin-bottom--lg">
            <div className="col text--center">
              <div style={{background: 'white'}} className="margin-bottom--sm">
                <a href="/press/vector-icon.svg">
                  <img src="/press/vector-icon.svg" width="100px" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-icon.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-icon.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
            <div className="col text--center">
              <div style={{background: 'black'}} className="margin-bottom--sm">
                <a href="/press/vector-icon.svg">
                  <img src="/press/vector-icon.svg" width="100px" height="auto" />
                </a>
              </div>
              <div>
                <a href="/press/vector-icon.svg"><i className="feather icon-download"></i> SVG</a>
                &nbsp;&nbsp;
                <a href="/press/vector-icon.png"><i className="feather icon-download"></i> PNG</a>
              </div>
            </div>
          </div>
        </section>
        <section>
          <AnchoredH2 id="connect">Diagrams</AnchoredH2>

          <AnchoredH3 id="connect">Components</AnchoredH3>

          <div className="text--center margin-bottom--sm">

            <div style={{background: 'white'}} className="margin-bottom--sm padding--lg">
              <a href="/press/vector-diagram-components.svg">
                <img src="/press/vector-diagram-components.svg" width="100%" height="auto" />
              </a>
            </div>
            <div>
              <a href="/press/vector-diagram-components.svg"><i className="feather icon-download"></i> SVG</a>
              &nbsp;&nbsp;
              <a href="/press/vector-diagram-components.png"><i className="feather icon-download"></i> PNG</a>
            </div>
          </div>
        </section>
        <section>
          <AnchoredH2 id="connect">Vector Blue</AnchoredH2>

          <div className="press--color">
            <div>Hex: #10E7FF</div>
            <div>RGB: 16, 231, 255</div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Contact;
