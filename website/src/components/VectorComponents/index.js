import React, {useState} from 'react';

import CheckboxList from '@site/src/components/CheckboxList';
import Jump from '@site/src/components/Jump';
import Link from '@docusaurus/Link';

import _ from 'lodash';
import classnames from 'classnames';
import humanizeString from 'humanize-string';
import qs from 'qs';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function Component({delivery_guarantee, description, event_types, name, status, type}) {
  let path = null;
  if(type == "source") path = `/docs/reference/sources/${name}/`;
  if(type == "transform") path = `/docs/reference/transforms/${name}/`;
  if(type == "sink") path = `/docs/reference/sinks/${name}/`;

  return (
    <Link to={path} className="vector-component">
      <div className="vector-component--header">
        {description && <i className="feather icon-info" title={description}></i>}
        <div className="vector-component--name">{name} {type}</div>
      </div>
      <div className="vector-component--badges">
        {event_types.includes("log") ?
          <span className="badge badge--primary" title="This component works with log event types">L</span> :
          ''}
        {event_types.includes("metric") ?
          <span className="badge badge--primary" title="This component works with metric event types">M</span> :
          ''}
        {status == "beta" ?
          <span className="badge badge--warning" title="This component is in beta and is not recommended for production environments"><i className="feather icon-alert-triangle"></i></span> :
          <span className="badge badge--primary" title="This component has passed reliability standards that make it production ready"><i className="feather icon-award"></i></span>}
        {delivery_guarantee == "best_effort" ?
          <span className="badge badge--warning" title="This component makes a best-effort delivery guarantee, and in rare cases can lose data"><i className="feather icon-shield-off"></i></span> :
          <span className="badge badge--primary" title="This component offers an at-least-once delivery guarantee"><i className="feather icon-shield"></i></span>}
      </div>
    </Link>
  );
}

function Components({components, headingLevel, titles}) {
  const sourceComponents = components.filter(component => component.type == "source");
  const transformComponents = components.filter(component => component.type == "transform");
  const sinkComponents = components.filter(component => component.type == "sink");
  const HeadingTag = `h${headingLevel || 3}`;

  if (components.length > 0) {
    return (
      <>
        {sourceComponents.length > 0 ?
          <>
            {titles && <HeadingTag>{sourceComponents.length} Sources</HeadingTag>}
            <div className="vector-components--grid">
              {sourceComponents.map((props, idx) => (
                <Component key={idx} {...props} />
              ))}
            </div>
          </>:
          ''}
        {transformComponents.length > 0 ?
          <>
            {titles && <HeadingTag>{transformComponents.length} Transforms</HeadingTag>}
            <div className="vector-components--grid">
              {transformComponents.map((props, idx) => (
                <Component key={idx} {...props} />
              ))}
            </div>
          </>:
          ''}
        {sinkComponents.length > 0 ?
          <>
            {titles && <HeadingTag>{sinkComponents.length} Sinks</HeadingTag>}
            <div className="vector-components--grid">
              {sinkComponents.map((props, idx) => (
                <Component key={idx} {...props} />
              ))}
            </div>
          </>:
          ''}
        <hr />
        <Jump to="https://github.com/timberio/vector/issues/new?labels=type%3A+new+feature" target="_blank" icon="plus-circle">
          Request a new component
        </Jump>
      </>
    );
  } else {
    return (
      <div className="empty">
        <div className="icon">☹</div>
        <div>No components found</div>
      </div>
    );
  }
}

function VectorComponents(props) {
  //
  // Base Variables
  //

  const {siteConfig} = useDocusaurusContext();
  const {metadata: {sources, transforms, sinks}} = siteConfig.customFields;
  const titles = props.titles || props.titles == undefined;
  const filterColumn = props.filterColumn == true;
  const queryObj = props.location ? qs.parse(props.location.search, {ignoreQueryPrefix: true}) : {};

  let components = [];
  if (props.sources || props.sources == undefined) components = components.concat(Object.values(sources));
  if (props.transforms || props.transforms == undefined) components = components.concat(Object.values(transforms));
  if (props.sinks || props.sinks == undefined) components = components.concat(Object.values(sinks));
  components = components.sort((a, b) => (a.name > b.name) ? 1 : -1);

  //
  // State
  //

  const [onlyAtLeastOnce, setOnlyAtLeastOnce] = useState(queryObj['at-least-once'] == 'true');
  const [onlyFunctions, setOnlyFunctions] = useState(new Set(queryObj['functions']));
  const [onlyLog, setOnlyLog] = useState(queryObj['log'] == 'true');
  const [onlyMetric, setOnlyMetric] = useState(queryObj['metric'] == 'true');
  const [onlyOperatingSystems, setOnlyOperatingSystems] = useState(new Set(queryObj['operating-systems']));
  const [onlyProductionReady, setOnlyProductionReady] = useState(queryObj['prod-ready'] == 'true');
  const [onlyProviders, setOnlyProviders] = useState(new Set(queryObj['providers']));
  const [searchTerm, setSearchTerm] = useState(null);

  //
  // Filtering
  //

  if (searchTerm) {
    components = components.filter(component => {
      let fullName = `${component.name.toLowerCase()} ${component.type.toLowerCase()}`;
      return fullName.includes(searchTerm.toLowerCase())
    });
  }

  if (onlyAtLeastOnce) {
    components = components.filter(component => component.delivery_guarantee == "at_least_once");
  }

  if (onlyFunctions.size > 0) {
    components = components.filter(component => onlyFunctions.has(component.function_category) );
  }

  if (onlyLog) {
    components = components.filter(component => component.event_types.includes("log"));
  }

  if (onlyMetric) {
    components = components.filter(component => component.event_types.includes("metric"));
  }

  if (onlyOperatingSystems.size > 0) {
    components = components.filter(component => Array.from(onlyOperatingSystems).every(x => component.operating_systems.includes(x)));
  }

  if (onlyProductionReady) {
    components = components.filter(component => component.status == "prod-ready");
  }

  if (onlyProviders.size > 0) {
    components = components.filter(component => onlyProviders.has(component.service_provider) );
  }

  //
  // Filter options
  //

  const operatingSystems = new Set(
    _(components).
      map(component => component.operating_systems).
      flatten().
      uniq().
      compact().
      sort().
      value());

  const serviceProviders = new Set(
    _(components).
      map(component => component.service_provider).
      uniq().
      compact().
      sort().
      value());

  const functionCategories = new Set(
    _(components).
      map(component => component.function_category).
      uniq().
      compact().
      sort().
      value());

  //
  // Rendering
  //

  return (
    <div className={classnames('vector-components', {'vector-components--cols': filterColumn})}>
      <div className="filters">
        <div className="search">
          <input
            type="text"
            onChange={(event) => setSearchTerm(event.currentTarget.value)}
            placeholder="🔍 Search..." />
        </div>
        <div className="filter">
          <div className="filter--label">
            <Link to="/docs/about/data-model/" title="Learn more about Vector's event types">
              Event types <i className="feather icon-info"></i>
            </Link>
          </div>
          <div className="filter--choices">
            <label title="Show only components that work with log event types.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyLog(event.currentTarget.checked)}
                checked={onlyLog} /> Log
            </label>
            <label title="Show only components that work with metric event types.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyMetric(event.currentTarget.checked)}
                checked={onlyMetric} /> Metric
            </label>
          </div>
        </div>
        <div className="filter">
          <div className="filter--label">
            <Link to="/docs/about/guarantees/" title="Learn more about Vector's guarantees">
              Guarantees <i className="feather icon-info"></i>
            </Link>
          </div>
          <div className="filter--choices">
            <label title="Show only components that offer an at-least-once delivery guarantee.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyAtLeastOnce(event.currentTarget.checked)}
                checked={onlyAtLeastOnce} />
              <i className="feather icon-shield"></i> At-least-once
            </label>
            <label title="Show only production ready components.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyProductionReady(event.currentTarget.checked)}
                checked={onlyProductionReady} />
              <i className="feather icon-award"></i> Prod-ready
            </label>
          </div>
        </div>
        <div className="filter">
          <div className="filter--label">Functions</div>
          <div className="filter--choices">
            <CheckboxList
              label="Functions"
              icon="code"
              values={functionCategories}
              currentState={onlyFunctions}
              setState={setOnlyFunctions} />
          </div>
        </div>
        {serviceProviders.size > 0 && (
          <div className="filter">
            <div className="filter--label">Providers</div>
            <div className="filter--choices">
              <CheckboxList
                label="Providers"
                icon="cloud"
                values={serviceProviders}
                currentState={onlyProviders}
                setState={setOnlyProviders} />
            </div>
          </div>
        )}
        {operatingSystems.size > 0 && (
          <div className="filter">
            <div className="filter--label">
              <Link to="/docs/setup/installation/operating-systems/" title="Learn more about Vector's operating systems">
                Operating Systems
              </Link>
            </div>
            <div className="filter--choices">
              <CheckboxList
                label="Operating Systems"
                icon="cpu"
                values={operatingSystems}
                currentState={onlyOperatingSystems}
                setState={setOnlyOperatingSystems} />
            </div>
          </div>
        )}
      </div>
      <div className="vector-components--results">
        <Components components={components} headingLevel={props.headingLevel} titles={titles} />
      </div>
    </div>
  );
}

export default VectorComponents;
