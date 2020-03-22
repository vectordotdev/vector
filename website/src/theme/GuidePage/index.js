import React, {useState} from 'react';

import CodeBlock from '@theme/CodeBlock';
import Heading from '@theme/Heading';
import InstallationCommand from '@site/src/components/InstallationCommand';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import Modal from 'react-modal';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import SVG from 'react-inlinesvg';
import Tags from '@site/src/components/Tags';
import VectorComponents from '@site/src/components/VectorComponents';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import readingTime from 'reading-time';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useTOCHighlight from '@theme/hooks/useTOCHighlight';

Modal.setAppElement('#__docusaurus')

const AnchoredH2 = Heading('h2');
const AnchoredH3 = Heading('h3');

const LINK_CLASS_NAME = 'contents__link';
const ACTIVE_LINK_CLASS_NAME = 'contents__link--active';
const TOP_OFFSET = 100;

/* eslint-disable jsx-a11y/control-has-associated-label */
function Headings({headings, isChild}) {
  if (!headings.length) return null;
  return (
    <ul className={isChild ? '' : 'contents contents__left-border'}>
      {headings.map(heading => (
        <li key={heading.id}>
          <a
            href={`#${heading.id}`}
            className={LINK_CLASS_NAME}
            dangerouslySetInnerHTML={{__html: heading.value}}
          />
          <Headings isChild headings={heading.children} />
        </li>
      ))}
    </ul>
  );
}

function SinkSwitcher() {
  return (
    <div className={styles.sinkSwitcher}>Switcher!</div>
  );
}

function GuidePage(props) {
  //
  // Props
  //

  const {content: GuideContents} = props;
  const {frontMatter, metadata} = GuideContents;
  const {category, id, platform_name: platformName, sink_name: sinkName, source_name: sourceName, title} = frontMatter;
  const {date: dateString, tags} = metadata;
  const readingStats = readingTime(GuideContents.toString());
  const date = new Date(Date.parse(dateString));

  //
  // Site config
  //

  const {siteConfig} = useDocusaurusContext();
  const {metadata: {installation, sources, sinks}} = siteConfig.customFields;
  const {platforms} = installation;
  const platform = platformName && platforms[platformName];
  const sink = sinkName && sinks[sinkName];
  const source = sourceName && sources[sourceName];
  const eventTypes = (platform || source || sink).event_types;

  let pathPrefix = '/guides/setup';

  if (platform) {
    pathPrefix = `/guides/setup/platforms/${platform.name}`;
  } else if (source) {
    pathPrefix = `/guides/setup/sources/${source.name}`;
  }

  //
  // State
  //

  const [showSinkSwitcher, setShowSinkSwitcher] = useState(false);

  //
  // Render
  //

  return (
    <Layout title="Collect Docker Logs & Send Them Anywhere" description="Collect Docker logs in minutes, for free. Quickly collect Docker logs and metrics and send them to one or more destinations.">
      {showSinkSwitcher && <Modal
        className="modal"
        onRequestClose={() => setShowSinkSwitcher(false)}
        overlayClassName="modal-overlay"
        isOpen={showSinkSwitcher}
        contentLabel="Minimal Modal Example">
          <header>
            <h1>Where do you want to send your data?</h1>
          </header>
          <VectorComponents
            eventTypes={eventTypes}
            pathPrefix={pathPrefix}
            titles={false}
            sources={false}
            transforms={false} />
      </Modal>}
      <header className="hero domain-bg domain-bg--platforms">
        <div className="container">
          {(platform || source || sink) && (
            <div className="component-icons">
              {platform && <div className="icon panel">
                {source.logo_path ?
                  <SVG src={source.logo_path} alt="Docker" /> :
                  <i className="feather icon-server"></i>}
              </div>}
              {source && !platform && <div className="icon panel">
                {source.logo_path ?
                  <SVG src={source.logo_path} alt="Docker" /> :
                  <i className="feather icon-server"></i>}
              </div>}
              {!source && !platform && <a href="#" className="icon panel" title="Select a source">
                <i className="feather icon-plus"></i>
              </a>}
              {sink && <a href="#" className="icon panel" title="Change your destination" onClick={(event) => setShowSinkSwitcher(true)}>
                {sink.logo_path ?
                  <SVG src={sink.logo_path} alt="Docker" /> :
                  <i className="feather icon-database"></i>}
               </a>}
               {!sink && <a href="#" className="icon panel" title="Select a destination" onClick={(event) => setShowSinkSwitcher(true)}>
                 <i className="feather icon-plus"></i>
               </a>}
            </div>
           )}
          <h1>{title}</h1>
          <div className={styles.credit}>Written, with <i className="feather icon-heart"></i>, by the <Link to="/community/#team">Vector team</Link>, last updated March 22, 2020</div>
          <Tags colorProfile="guides" tags={tags} />
        </div>
      </header>
      <main className="container container--narrow margin-vert--xl">
        <section className="markdown align-text-edges">
          <MDXProvider components={MDXComponents}><GuideContents /></MDXProvider>
        </section>
      </main>
    </Layout>
  );
}

export default GuidePage;
