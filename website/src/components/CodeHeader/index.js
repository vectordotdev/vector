import React from 'react';

import './styles.css';

function CodeHeader({icon, text}) {
  return (
    <div className="code-header">
      {icon && <><i className="feather icon-info"></i> </>}
      {text}
    </div>
  );
}

export default CodeHeader;
