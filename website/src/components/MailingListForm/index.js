import React from 'react';

import classnames from 'classnames';

import './styles.css';

function MailingListForm({block, buttonClass, center, description, size, width}) {
  return (
    <div className={classnames('mailing-list', {'mailing-list--block': block, 'mailing-list--center': center, [`mailing-list--${size}`]: size})}>
      {description !== false && (
        <div className="mailing-list--description">
          The easiest way to stay up-to-date. One email on the 1st of every month. No spam, ever.
        </div>
      )}
      <form action="https://app.getvero.com/forms/a748ded7ce0da69e6042fa1e21042506" method="post" className="mailing-list--form">
        <input className={classnames('input', `input--${size}`)} name="email" placeholder="you@email.com" type="email" style={{width: width}} />
        <button className={classnames('button', `button--${buttonClass || 'primary'}`, `button--${size}`)} type="submit">Subscribe</button>
      </form>
    </div>
  );
}

export default MailingListForm;
