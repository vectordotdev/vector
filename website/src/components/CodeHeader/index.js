import React from 'react';

import './styles.css';

function CodeHeader({fileName, links}) {
  let linkElements = [];

  for (var link in links) {
    linkElements.push(<a href={link.href}>{link.text || "Learn more&hellip;"}</a>);
  }

  return (
    <div className="code-header">
      {linkElements.length > 0 &&
        <span className="code-header--links">{linkElements}</span>}
      {fileName}
    </div>
  );
}

export default CodeHeader;
