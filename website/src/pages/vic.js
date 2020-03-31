import React from 'react';

import Layout from '@theme/Layout';

import classnames from 'classnames';
import styles from './vic.module.css';

function Vic() {
  return (
    <Layout title="Vic" description="The offical Vector mascot">
      <header className={classnames("hero padding-bottom--sm", styles.vicHero)}>
        <div className="container container--fluid">
          <div className="row">
            <div className="col text--center">
              <h1>Vic the Vector Squirrel</h1>
              <div className="hero--subtitle">
                The official Vector mascot.
              </div>
            </div>
          </div>
          <div className="row margin-bottom--md margin-top--lg">
            <div className="col text--center">
              <div className="margin-bottom--sm">
                <a href="/img/vic.svg">
                  <img src="/img/vic.svg" className={styles.vicLarge} width="100%" height="auto" />
                </a>
              </div>
            </div>
          </div>
        </div>
      </header>
      <main>
        <section>
          <div className="container container-narrow padding-top--lg">
            <div className="row margin-bottom--lg">
              <div className="col col--8 col--offset-2">
                <p>
Vic was a vagrant, truly living life on the edge. A flying squirrel, the perfect
form factor for a cat burglar (except maybe for a cat), Vic was infamous among
curators of rare nuts world wide. If you had a nut worth taking you'd best be
ready, and none ever were.
                </p>
                <p>
Treasuries, galleries and private collectors alike would spend fortunes
attempting to protect their precious cargo, to no avail. That is, until it all
came crumbling down for Vic in one fateful night.
                </p>
                <p>
It was a typical run-of-the-mill operation. Vic had planned to extract the
famous Allnatt Acorn from the Smithsoninut Museum. However, as fate would have
it Vic's escape plan was foiled from the get go. The web service responsible for
dispatching a fake distress call had actually been broken for weeks without Vic
noticing. As a result, there was nothing to distract the guards and Vic's escape
was blocked.
                </p>
                <p>
Vic is just a squirrel and so there were no consequences for being caught.
However, the experience left such a bitter taste that Vic couldn't let it go.
Since that day Vic has been dedicated to improving observability infrastructure
by building dope tooling and by establishing dapper new best practices. No cat
burglarizing squirrel should ever need to suffer again.
                </p>
              </div>
            </div>
          </div>
        </section>
        <section>
          <div className="container container--xs">
            <div className="row">
              <div className="col text--center">
                <h3 className="margin-bottom--sm">Emojis</h3>
                <p>Become the hero of your slack org by adding these fresh Vic emojis.</p>
              </div>
            </div>
            <div className="row margin-bottom--lg">
              <div className="col text--center">
                <a href="/img/vicmojis/vic.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/vic.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/vicok.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/vicok.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/vicyes.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/vicyes.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/vicno.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/vicno.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/viccool.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/viccool.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/victhinking.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/victhinking.png"/>
                </a>
              </div>
              <div className="col text--center">
                <a href="/img/vicmojis/vicheart.png">
                  <img className={styles.vicmoji} src="/img/vicmojis/vicheart.png"/>
                </a>
              </div>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

export default Vic;
