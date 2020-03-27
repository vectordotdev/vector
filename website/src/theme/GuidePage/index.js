import React, {useState} from 'react';

import Avatar from '@site/src/components/Avatar';
import CodeBlock from '@theme/CodeBlock';
import Heading from '@theme/Heading';
import InstallationCommand from '@site/src/components/InstallationCommand';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import Modal from 'react-modal';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import PagePaginator from '@theme/PagePaginator';
import SVG from 'react-inlinesvg';
import Tags from '@site/src/components/Tags';
import VectorComponents from '@site/src/components/VectorComponents';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
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
    <ul className={isChild ? '' : 'contents'}>
      {!isChild && (
        <li>
          <a
            href="#overview"
            className={LINK_CLASS_NAME}>
            Overview
          </a>
        </li>
      )}
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

function GuidePage(props) {
  //
  // Props
  //

  const {content: GuideContents} = props;
  const {frontMatter, metadata} = GuideContents;
  const {author_github: authorGithub, id, last_modified_on: lastModifiedOn, series_position: seriesPosition, title} = frontMatter;
  const {categories, readingTime, tags} = metadata;

  //
  // Site config
  //

  const {siteConfig} = useDocusaurusContext();
  const {metadata: {guides: guidesMetadata, installation, sources, sinks}} = siteConfig.customFields;
  const {platforms} = installation;
  const category = guidesMetadata[categories[0]];

  //
  // Variables
  //

  const enrichedTags = enrichTags(tags, 'guides');
  const domainTag = enrichedTags.find(tag => tag.category == 'domain');
  const domainBG = domainTag ? domainTag.value : 'default';
  const lastModified = Date.parse(lastModifiedOn);
  const platformTag = enrichedTags.find(tag => tag.category == 'platform');
  const platformName = platformTag ? platformTag.value : null;
  const platform = platformName && platforms[platformName];
  const sinkTag = enrichedTags.find(tag => tag.category == 'sink');
  const sinkName = sinkTag ? sinkTag.value : null;
  const sink = sinkName && sinks[sinkName];
  const sourceTag = enrichedTags.find(tag => tag.category == 'source');
  const sourceName = sourceTag ? sourceTag.value : null;
  const source = sourceName && sources[sourceName];
  const eventTypes = (source || sink || {}).event_types || [];

  let pathPrefix = '/guides/setup';

  if (platform) {
    pathPrefix = `/guides/setup/platforms/${platform.name}`;
  } else if (source) {
    pathPrefix = `/guides/setup/sources/${source.name}`;
  }

  //
  // State
  //

  const [showComponentSwitcher, setShowComponentSwitcher] = useState(null);

  //
  // Render
  //

  useTOCHighlight(LINK_CLASS_NAME, ACTIVE_LINK_CLASS_NAME, TOP_OFFSET);

  return (
    <Layout title={title} description={`${title}, in minutes, for free`}>
      {showComponentSwitcher && <Modal
        className="modal"
        onRequestClose={() => setShowComponentSwitcher(false)}
        overlayClassName="modal-overlay"
        isOpen={showComponentSwitcher !== null}
        contentLabel="Minimal Modal Example">
          <header>
            <h1>Where do you want to send your data?</h1>
          </header>
          <VectorComponents
            exceptFunctions={['test']}
            exceptNames={[source && source.name]}
            eventTypes={eventTypes}
            pathPrefix={pathPrefix}
            titles={false}
            sources={false}
            transforms={false} />
      </Modal>}
      <header className={`hero domain-bg domain-bg--${domainBG}`}>
        <div className="container">
          {(platform || source || sink) && (
            <div className="component-icons">
              {platform && <div className="icon panel">
                {platform.logo_path ?
                  <SVG src={platform.logo_path} alt={`${platform.title} Logo`} /> :
                  <i className="feather icon-server"></i>}
              </div>}
              {source && !platform && <div className="icon panel link" title="Change your source" onClick={(event) => setShowComponentSwitcher('source')}>
                {source.logo_path ?
                  <SVG src={source.logo_path} alt={`${source.title} Logo`} /> :
                  <i className="feather icon-server"></i>}
              </div>}
              {!source && !platform && <div className="icon panel link" title="Select a source" onClick={(event) => setShowComponentSwitcher('source')}>
                 <i className="feather icon-plus"></i>
               </div>}
              {!source && !platform && <div className="icon panel link" title="Select a source">
                <i className="feather icon-plus"></i>
              </div>}
              {sink && <div className="icon panel link" title="Change your destination" onClick={(event) => setShowComponentSwitcher('sink')}>
                {sink.logo_path ?
                  <SVG src={sink.logo_path} alt={`${sink.title} Logo`} /> :
                  <i className="feather icon-database"></i>}
               </div>}
               {!sink && <div className="icon panel link" title="Select a destination" onClick={(event) => setShowComponentSwitcher('sink')}>
                 <i className="feather icon-plus"></i>
               </div>}
            </div>
          )}
          {(!platform && !source && !sink) && (
            <div className="hero--category"><Link to={`/guides/${category.name}/`}>{category.title}{category.series && <> Series - {seriesPosition} of {category.guides.length}</>}</Link></div>)}
          <h1 className={styles.header}>{title}</h1>
          <div className="hero--subtitle">{frontMatter.description}</div>
          <Tags colorProfile="guides" tags={tags} />
        </div>
      </header>
      <main className={classnames('container', 'container--l', styles.container)}>
        <div className="row">
          <aside className="col col--2-5">
            <section className={styles.avatar}>
              <Avatar
                bio={true}
                github={authorGithub}
                size="lg"
                rel="author"
                subTitle={false}
                vertical={true} />
            </section>
            <section className={classnames('table-of-contents', styles.tableOfContents)}>
              <div className="section">
                <div className="title">Stats</div>

                <div className="text--secondary text--bold"><i className="feather icon-book"></i> {readingTime}</div>
                <div className="text--secondary text--bold"><i className="feather icon-clock"></i> Updated <time pubdate="pubdate" dateTime={lastModifiedOn}>{dateFormat(lastModified, "mmm dS, yyyy")}</time></div>
              </div>
              {GuideContents.rightToc.length > 0 && (
                <div className="section">
                  <div className="title">Contents</div>
                  <Headings headings={GuideContents.rightToc} />
                </div>
              )}
            </section>
          </aside>
          <div className={classnames('col', styles.rightCol)}>
            <article>
              <div className="markdown">
                <a aria-hidden="true" tabindex="-1" class="anchor" id="overview"></a>
                <MDXProvider components={MDXComponents}><GuideContents /></MDXProvider>
              </div>
            </article>
            <PagePaginator previous={metadata.previousItem} next={metadata.nextItem} className={styles.paginator} />
          </div>
        </div>
      </main>
    </Layout>
  );
}

export default GuidePage;
