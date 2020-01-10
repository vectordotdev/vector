import React from 'react';

import './styles.css';

function Steps({children, type}) {
  return (
    <ol className="steps">
      {children}
    </ol>
  );
}

export default Steps;
