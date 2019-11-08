import React from 'react';

import './styles.css';

function Component({delivery_guarantee, description, event_types, name, path, status, type}) {
  return (
    <a href={path} className="component">
      <div className="component-header">
        {description && <i className="feather icon-info" title={description}></i>}
        <div className="component-name">{name} {type}</div>
      </div>
      <div className="badges">
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
    </a>
  );
}

export default Component;