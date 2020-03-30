import React from 'react';

import humanizeString from 'humanize-string';

import './styles.css';

function CheckboxList({humanize, icon, values, currentState, setState}) {
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
              type="checkbox"
              onChange={(event) => {
                let newValues = new Set(currentState);

                if (event.currentTarget.checked)
                  newValues.add(value);
                else
                  newValues.delete(value);

                setState(newValues);
              }}
              checked={currentState.has(value)} />
            {label && <>{icon ? <i className={`feather icon-${icon}`}></i> : ''} {label}</>}
          </label>
        );
      })}
    </>
  );
}

export default CheckboxList;
