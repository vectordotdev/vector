import React from 'react';

import './styles.css';

function CodeHeader({fileName, learnMoreUrl}) {
  return (
    <div className="code-header">
      {learnMoreUrl && <a href={learnMoreUrl} className="learn-more" title="Learn more about configuring Vector"><i className="feather icon-info"></i> learn more</a>}
      {fileName}
    </div>
  );
}

export default CodeHeader;
