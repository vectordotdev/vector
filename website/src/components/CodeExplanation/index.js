import React, { useState } from 'react';

import Alert from '@site/src/components/Alert';

import './styles.css';

function CodeExplanation({children}) {
  const [isToggled, setToggled] = useState(false);

  if (isToggled) {
    return (
      <div className="code-explanation code-explanation--expanded">
        {children}

        <div className="code-explanation--toggle" onClick={() => setToggled(!isToggled)}>
          <i className="feather icon-arrow-up-circle"></i> hide
        </div>
      </div>
    );
  } else {
    return (
      <div className="code-explanation code-explanation--collapsed">
        <div className="code-explanation--toggle" onClick={() => setToggled(!isToggled)}>
          <i className="feather icon-info"></i> explain this command
        </div>
      </div>
    );
   }
}

export default CodeExplanation;
