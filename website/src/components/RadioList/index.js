import React from 'react';

import humanizeString from 'humanize-string';

function RadioList({currentState, humanize, icon, name, setState, values}) {
  if (values.size == 0)
    return null;

  let valuesArr = Array.from(values)

  return (
    <>
      {valuesArr.map((value, idx) => {
        let label = (typeof value === 'string' && humanize) ? humanizeString(value) : value;

        return (
          <label key={idx}>
            <input
              checked={value == currentState}
              name={name}
              onChange={(event) => setState(value)}
              type="radio"
               />
            {label && <>{icon ? <i className={`feather icon-${icon}`}></i> : ''} {label}</>}
          </label>
        );
      })}
    </>
  );
}

export default RadioList;
