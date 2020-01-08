import React from 'react';

import './styles.css';

function Step({children}) {
  return (
    <li className="step">
      {children}
    </li>
  );
}

export default Step;
