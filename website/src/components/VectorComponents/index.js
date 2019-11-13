import React, {useState} from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function Component({delivery_guarantee, description, event_types, name, status, type}) {
  let path = null;
  if(type == "source") path = `/docs/components/sources/${name}`;
  if(type == "transform") path = `/docs/components/transforms/${name}`;
  if(type == "sink") path = `/docs/components/sinks/${name}`;

  return (
    <Link to={path} className="vector-component">
      <div className="vector-component--header">
        {description && <i className="feather icon-info" title={description}></i>}
        <div className="vector-component--name">{name} {type}</div>
      </div>
      <div className="vector-component--badges">
        {event_types.includes("log") ?
          <span className="badge badge--secondary" title="This component works with log event types"><i className="feather icon-database"></i> log</span> :
          ''}
        {event_types.includes("metric") ?
          <span className="badge badge--secondary" title="This component works with metric event types"><i className="feather icon-bar-chart"></i> metric</span> :
          ''}
        {status == "beta" ?
          <span className="badge badge--warning" title="This component is in beta and is not recommended for production environments"><i className="feather icon-alert-triangle"></i> beta</span> :
          <span className="badge badge--primary" title="This component has passed reliability standards that make it production ready"><i className="feather icon-award"></i> prod-ready</span>}
        {delivery_guarantee == "best_effort" ?
          <span className="badge badge--warning" title="This component makes a best-effort delivery guarantee, and in rare cases can lose data"><i className="feather icon-shield-off"></i> best-effort</span> :
          <span className="badge badge--primary" title="This component offers an at-least-once delivery guarantee"><i className="feather icon-shield"></i> at-least-once</span>}
      </div>
    </Link>
  );
}

function VectorComponents(props) {
  const {siteConfig} = useDocusaurusContext();
  const {metadata: {sources, transforms, sinks}} = siteConfig.customFields;
  const [onlyAtLeastOnce, setOnlyAtLeastOnce] = useState(false);
  const [onlyLog, setOnlyLog] = useState(false);
  const [onlyMetric, setOnlyMetric] = useState(false);
  const [onlyProductionReady, setOnlyProductionReady] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);
  const titles = props.titles || props.titles == undefined;
  const filterColumn = props.filterColumn == true;
  const HeadingTag = `h${props.headingLevel || 3}`;

  let components = [];
  if (props.sources || props.sources == undefined) components = components.concat(Object.values(sources));
  if (props.transforms || props.transforms == undefined) components = components.concat(Object.values(transforms));
  if (props.sinks || props.sinks == undefined) components = components.concat(Object.values(sinks));
  components = components.sort((a, b) => (a.name > b.name) ? 1 : -1);

  if (onlyAtLeastOnce) {
    components = components.filter(component => component.delivery_guarantee == "at_least_once");
  }

  if (onlyLog) {
    components = components.filter(component => component.event_types.includes("log"));
  }

  if (onlyMetric) {
    components = components.filter(component => component.event_types.includes("metric"));
  }

  if (onlyProductionReady) {
    components = components.filter(component => component.status == "prod-ready");
  }

  if (searchTerm) {
    components = components.filter(component => {
      let fullName = `${component.name.toLowerCase()} ${component.type.toLowerCase()}`;
      return fullName.includes(searchTerm.toLowerCase())
    });
  }

  const sourceComponents = components.filter(component => component.type == "source");
  const transformComponents = components.filter(component => component.type == "transform");
  const sinkComponents = components.filter(component => component.type == "sink");

  let serviceProvers =
    components.filter(component => component.service_provider).
    map(component => component.service_provider).
    sort((a, b) => (a > b) ? 1 : -1);

  console.log(components)
  serviceProvers = new Set(serviceProvers);
  serviceProvers = Array.from(serviceProvers);

  return (
    <div className={classnames('vector-components', {'vector-components--cols': filterColumn})}>
      <div className="vector-components--filters">
        <div className="vector-components--filters--search">
          <input
            type="text"
            onChange={(event) => setSearchTerm(event.currentTarget.value)}
            placeholder="🔍 Search..." />
        </div>
        <div className="vector-components--filters--checkboxes">
          <span className="vector-components--filters--section">
            <div className="vector-components--filters--section--title">
              <Link to="/docs/about/data-model" title="Learn more about Vector's event types">
                Event types <i className="feather icon-info"></i>
              </Link>
            </div>
            <label title="Show only components that work with log event types.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyLog(event.currentTarget.checked)}
                checked={onlyLog} />
              Log <i className="feather icon-database"></i>
            </label>
            <label title="Show only components that work with metric event types.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyMetric(event.currentTarget.checked)}
                checked={onlyMetric} />
              Metric <i className="feather icon-bar-chart"></i>
            </label>
          </span>
          <span className="vector-components--filters--section">
            <div className="vector-components--filters--section--title">
              <Link to="/docs/about/guarantees" title="Learn more about Vector's guarantees">
                Guarantees <i className="feather icon-info"></i>
              </Link>
            </div>
            <label title="Show only components that offer an at-least-once delivery guarantee.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyAtLeastOnce(event.currentTarget.checked)}
                checked={onlyAtLeastOnce} />
              At-least-once <i className="feather icon-shield"></i>
            </label>
            <label title="Show only production ready components.">
              <input
                type="checkbox"
                onChange={(event) => setOnlyProductionReady(event.currentTarget.checked)}
                checked={onlyProductionReady} />
              Prod-ready <i className="feather icon-award"></i>
            </label>
          </span>
          {serviceProvers.length > 0 ? (
            <span className="vector-components--filters--section">
              <div className="vector-components--filters--section--title">
                Service Providers
              </div>
              {serviceProvers.map((serviceProver, idx) => (
                <label key={idx} title={`Show only components from the ${serviceProver} provider.`}>
                  <input
                    type="checkbox"
                    onChange={(event) => {
                      setOnlyAtLeastOnce(event.currentTarget.checked)
                    }}
                    checked={onlyAtLeastOnce} />
                  {serviceProver} <i className="feather icon-shield"></i>
                </label>
              ))}
            </span>) :
            null}
        </div>
      </div>
      <div className="vector-components--results">
        {components.length > 0 ?
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
          </> :
          <div className="empty">
            <div className="icon">☹</div>
            <div>No components found</div>
          </div>}
      </div>
    </div>
  );
}

export default VectorComponents;