window.addEventListener('load', function () {
    const { env } = document.documentElement.dataset;

    if (env !== 'development') {
        // add trustarc script to head
        const trustarc = document.createElement('script');
        trustarc.setAttribute('src','https://consent.trustarc.com/v2/notice/ufocto');
        document.head.appendChild(trustarc);

        // add divs
        const divA = document.createElement("div");
        const divB = document.createElement("div");
        divA.id = "teconsent";
        divA.style = "cursor: pointer; color:#fff"
        divB.id = "consent-banner";
        divB.style = "position:fixed; bottom:0px; right:0px; width:100%;";
        document.body.appendChild(divA);
        document.body.appendChild(divB);

        // update Cookie link
        this.setTimeout(function () {
            const banner = document.getElementById('consent-banner');
            const prefsElement = document.getElementById('teconsent');
            const cookieLink = document.querySelector('footer a[href*="/cookies"]');
            prefsElement.className = cookieLink.className;

            if (banner) {
                // listen for click and remove banner to avoid interfering with
                document.addEventListener('click', function (event) {
                    const targetElement = event.target;
                    if (targetElement.matches('#truste-consent-required') || targetElement.matches('#truste-consent-button')) {
                        banner.remove();
                    }
                });
            }

            // replace Cookie link with Prefs div
            return (cookieLink && document.getElementById('teconsent').innerHTML.length > 0) ? cookieLink.replaceWith(prefsElement) : false;
        }, 200);
    }
});
