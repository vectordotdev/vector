import React from 'react';

import Head from '@docusaurus/Head';
import Link from '@docusaurus/Link';
import PagePaginator from '@theme/PagePaginator';

import _ from 'lodash';
import classnames from 'classnames';
import styles from './styles.module.css';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useTOCHighlight from '@theme/hooks/useTOCHighlight';

const LINK_CLASS_NAME = 'contents__link';
const ACTIVE_LINK_CLASS_NAME = 'contents__link--active';
const TOP_OFFSET = 100;

function Headings({headings, isChild}) {
  useTOCHighlight(LINK_CLASS_NAME, ACTIVE_LINK_CLASS_NAME, TOP_OFFSET);

  if (!headings.length) return null;

  // We need to track shown headings because the markdown parser will
  // extract duplicate headings if we're using tabs
  let uniqHeadings = _.uniqBy(headings, (heading => heading.value));


  return (
    <ul className={isChild ? '' : 'contents'}>
      {uniqHeadings.map(heading => {
        let cleanValue = heading.value.replace('<code><', '<code>&lt;').replace('></code>', '&gt;</code>');

        return <li key={heading.id}>
          <a
            href={`#${heading.id}`}
            className={LINK_CLASS_NAME}
            dangerouslySetInnerHTML={{__html: cleanValue}}
          />
          <Headings isChild headings={heading.children} />
        </li>
      })}
    </ul>
  );
}

function SupportedEventTypes({values}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {event_types: eventTypes}} = siteConfig.customFields;

  let els = [];

  eventTypes.forEach(eventType => {
    if (values.includes(eventType)) {
      els.push(<span key={eventType} className="text--primary">{_.capitalize(eventType)}</span>);
    } else {
      els.push(<del key={eventType} className="text--warning">{_.capitalize(eventType)}</del>);
    }
    els.push(<span key={eventType + '-comma'}>, </span>);
  });

  els.pop();

  return els;
}

function OperatingSystemsStatus({operatingSystems, unsupportedOperatingSystems}) {
  let operatingSystemsEls = [];

  (operatingSystems || []).forEach(operatingSystem => {
    operatingSystemsEls.push(<span key={operatingSystem} className="text--primary">{operatingSystem}</span>);
    operatingSystemsEls.push(<span key={operatingSystem + '-comma'}>, </span>);
  });

  (unsupportedOperatingSystems || []).forEach(operatingSystem => {
    operatingSystemsEls.push(<del key={operatingSystem} className="text--warning">{operatingSystem}</del>);
    operatingSystemsEls.push(<span key={operatingSystem + '-comma'}>, </span>);
  });

  operatingSystemsEls.pop();

  return operatingSystemsEls;
}

function Statuses({deliveryGuarantee, eventTypes, operatingSystems, status, unsupportedOperatingSystems}) {
  if (!status && !deliveryGuarantee && !operatingSystems && !unsupportedOperatingSystems)
    return null;

  return (
    <div className="section">
      <div className="title">Support</div>
      {status == "beta" &&
        <div>
          <Link to="/docs/about/guarantees/#beta" className="text--warning" title="This component is in beta and is not recommended for production environments. Click to learn more.">
            <i className="feather icon-alert-triangle"></i> Beta Status
          </Link>
        </div>}
      {status == "prod-ready" &&
        <div>
          <Link to="/docs/about/guarantees/#prod-ready" className="text--primary" title="This component has passed reliability standards that make it production ready. Click to learn more.">
            <i className="feather icon-award"></i> Prod-Ready Status
          </Link>
        </div>}
      {deliveryGuarantee == "best_effort" &&
        <div>
          <Link to="/docs/about/guarantees/#best-effort" className="text--warning" title="This component makes a best-effort delivery guarantee, and in rare cases can lose data. Click to learn more.">
            <i className="feather icon-shield-off"></i> Best-Effort Delivery
          </Link>
        </div>}
      {deliveryGuarantee == "at_least_once" &&
        <div>
          <Link to="/docs/about/guarantees/#at-least-once" className="text--primary" title="This component offers an at-least-once delivery guarantee. Click to learn more.">
            <i className="feather icon-shield"></i> At-Least-Once
          </Link>
        </div>}
      {eventTypes &&
        <div>
          <Link to="/docs/about/data-model/" title={`This component works on the these event types.`}>
            <i className="feather icon-database"></i> <SupportedEventTypes values={eventTypes} />
          </Link>
        </div>}
      {operatingSystems && unsupportedOperatingSystems &&
        <div>
          <Link to="/docs/setup/installation/operating-systems/" title={`This component works on the ${operatingSystems.join(", ")} operating systems.`}>
            <i className="feather icon-cpu"></i> <OperatingSystemsStatus operatingSystems={operatingSystems} unsupportedOperatingSystems={unsupportedOperatingSystems} />
          </Link>
        </div>}
    </div>
  );
}

function DocItem(props) {
  const {siteConfig = {}} = useDocusaurusContext();
  const {title: siteTitle, url: siteUrl} = siteConfig;
  const {content: DocContent} = props;
  const {metadata} = DocContent;

  const {
    description,
    editUrl,
    image: metaImage,
    keywords,
    lastUpdatedAt,
    lastUpdatedBy,
    permalink,
    title,
    version
  } = metadata;
  const {
    frontMatter: {
      component_title: componentTitle,
      delivery_guarantee: deliveryGuarantee,
      event_types: eventTypes,
      function_category: functionCategory,
      hide_title: hideTitle,
      hide_table_of_contents: hideTableOfContents,
      issues_url: issuesUrl,
      operating_systems: operatingSystems,
      posts_path: postsPath,
      source_url: sourceUrl,
      status,
      unsupported_operating_systems: unsupportedOperatingSystems,
    },
  } = DocContent;

  const metaImageUrl = siteUrl + useBaseUrl(metaImage);

  return (
    <div>
      <Head>
        {title && <title>{title} | Docs | {siteTitle}</title>}
        {description && <meta name="description" content={description} />}
        {description && (
          <meta property="og:description" content={description} />
        )}
        {keywords && keywords.length && (
          <meta name="keywords" content={keywords.join(',')} />
        )}
        {metaImage && <meta property="og:image" content={metaImageUrl} />}
        {metaImage && <meta property="twitter:image" content={metaImageUrl} />}
        {metaImage && (
          <meta name="twitter:image:alt" content={`Image for ${title}`} />
        )}
        {permalink && <meta property="og:url" content={siteUrl + permalink} />}
      </Head>
      <div className={styles.container}>
        <div className={styles.leftCol}>
          <div className="docItemContainer_">
            <article>
              {version && (
                <span
                  style={{verticalAlign: 'top'}}
                  className="badge badge--info">
                  Version: {version}
                </span>
              )}

              {!metadata.hide_title && (
                <header>
                  <div className="badges">
                    {functionCategory && <Link to={`/components?functions[]=${functionCategory}`} className="badge badge--primary">{functionCategory}</Link>}
                  </div>
                  <h1 className={styles.docTitle}>{metadata.title}</h1>
                </header>
              )}

              <div className="markdown">
                <DocContent />
              </div>
            </article>
          </div>
          {!metadata.hide_pagination && (metadata.next || metadata.previous) && (
            <div className={styles.paginator}>
              <PagePaginator next={metadata.next} previous={metadata.previous} />
            </div>
          )}
        </div>
        {DocContent.rightToc && (
          <div className={styles.rightCol}>
            <div className={classnames('table-of-contents', styles.tableOfContents)}>
              <Statuses
                deliveryGuarantee={deliveryGuarantee}
                eventTypes={eventTypes}
                operatingSystems={operatingSystems}
                status={status}
                unsupportedOperatingSystems={unsupportedOperatingSystems} />
              {DocContent.rightToc.length > 0 &&
                <div className="section">
                  <div className="title">Contents</div>
                  <Headings headings={DocContent.rightToc} />
                </div>
              }
              <div className="section">
                <div className="title">Resources</div>
                <ul className="contents">
                  {editUrl && (<li><a href={editUrl} className="contents__link" target="_blank"><i className="feather icon-edit-1"></i> Edit this page</a></li>)}
                  {postsPath && (<li><Link to={postsPath} className="contents__link"><i className="feather icon-book-open"></i> View Blog Posts</Link></li>)}
                  {issuesUrl && (<li><a href={issuesUrl} className="contents__link" target="_blank"><i className="feather icon-message-circle"></i> View Issues</a></li>)}
                  {sourceUrl && (<li><a href={sourceUrl} className="contents__link" target="_blank"><i className="feather icon-github"></i> View Source</a></li>)}
                </ul>
              </div>
              {(lastUpdatedAt || lastUpdatedBy) && (
                <div className="section">
                  Last updated{' '}
                  {lastUpdatedAt && (
                    <>
                      on{' '}
                      <strong>
                        {new Date(
                          lastUpdatedAt * 1000,
                        ).toLocaleDateString()}
                      </strong>
                      {lastUpdatedBy && ' '}
                    </>
                  )}
                  {lastUpdatedBy && (
                    <>
                      by <strong>{lastUpdatedBy}</strong>
                    </>
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default DocItem;
