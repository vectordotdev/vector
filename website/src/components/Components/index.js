import React, {useState} from 'react';

import './styles.css';

function Components({children}) {
  const [onlyAtLeastOnce, setOnlyAtLeastOnce] = useState(false);
  const [onlyLog, setOnlyLog] = useState(false);
  const [onlyMetric, setOnlyMetric] = useState(false);
  const [onlyProductionReady, setOnlyProductionReady] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);

  let filteredChildren = children;

  if (onlyAtLeastOnce) {
    filteredChildren = filteredChildren.filter(child => child.props.delivery_guarantee == "at_least_once");
  }

  if (onlyLog) {
    filteredChildren = filteredChildren.filter(child => child.props.event_types.includes("log"));
  }

  if (onlyMetric) {
    filteredChildren = filteredChildren.filter(child => child.props.event_types.includes("metric"));
  }

  if (onlyProductionReady) {
    filteredChildren = filteredChildren.filter(child => child.props.status == "prod-ready");
  }

  if (searchTerm) {
    filteredChildren = filteredChildren.filter(child => {
      let fullName = `${child.props.name.toLowerCase()} ${child.props.type.toLowerCase()}`;
      return fullName.includes(searchTerm.toLowerCase())
    });
  }

  let sources = filteredChildren.filter(child => child.props.type == "source");
  let transforms = filteredChildren.filter(child => child.props.type == "transform");
  let sinks = filteredChildren.filter(child => child.props.type == "sink");

  return (
    <div className="components">
      <div className="filters">
        <div className="search">
          <input
            type="text"
            onChange={(event) => setSearchTerm(event.currentTarget.value)}
            placeholder="🔍 Search..." />
        </div>
        <div className="checkboxes">
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
        </div>
      </div>
      <div className="component-cards">
        {!Array.isArray(filteredChildren) || filteredChildren.length > 0 ?
          <div>
            {!Array.isArray(sources) || sources.length > 0 ?
              <div>
                <h3>{sources.length} Sources</h3>
                <div className="component-cards">
                  {sources}
                </div>
              </div>:
              ''}
            {!Array.isArray(transforms) || transforms.length > 0 ?
              <div>
                <h3>{transforms.length} Transforms</h3>
                <div className="component-cards">
                  {transforms}
                </div>
              </div>:
              ''}
            {!Array.isArray(sinks) || sinks.length > 0 ?
              <div>
                <h3>{sinks.length} Sinks</h3>
                <div className="component-cards">
                  {sinks}
                </div>
              </div>:
              ''}
          </div> :
          <div className="empty">
            <div className="icon">☹</div>
            <div>No components found</div>
          </div>}
      </div>
    </div>
  );
}

export default Components;