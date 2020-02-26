import React, {useState, Children} from 'react';

import classnames from 'classnames';

import './styles.css';

function Accordion({children}) {
  const [selectedLabel, setSelectedLabel] = useState(null);

  return (
    <ul className="accordion">
      {
        Children.toArray(children).map((child, idx) => {
          let label = child.props.label;
          let expanded = label == selectedLabel;

          return (
            <li className={classnames('accordion-item', `accordion-item--${expanded ? 'expanded' : 'collapsed'}`)}>
              <div className="accordion-label" onClick={() => setSelectedLabel(label)}>
                <i className={`feather icon icon-${expanded ? 'minus' : 'plus'}-circle`}></i> {label}
              </div>
              {label == selectedLabel && (
                <div className="accordion-body">
                  {child.props.children}
                </div>
              )}
            </li>
          );
        })
      }
    </ul>
  );
}

export default Accordion;
