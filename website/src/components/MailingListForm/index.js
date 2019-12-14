import React from 'react';

import classnames from 'classnames';

import './styles.css';

function MailingListForm({block, buttonClass, description, size}) {
  return (
    <div className={classnames('mailing-list', `mailing-list--${size}`, {'mailing-list--block': block})}>
      {description !== false && (
        <div className="mailing-list--description">
          The easiest way to stay up-to-date. One email on the 1st of every month. No spam, ever.
        </div>
      )}
      <form action="https://app.getvero.com/forms/a748ded7ce0da69e6042fa1e21042506" method="post">
        <div className="subscribe_form">
          <input className={classnames('input', `input--${size}`)} name="email" placeholder="you@email.com" type="email" />
          <button className={classnames('button', `button--${buttonClass || 'primary'}`, `button--${size}`)} type="submit">Subscribe</button>
        </div>
      </form>
    </div>
  );
}

export default MailingListForm;