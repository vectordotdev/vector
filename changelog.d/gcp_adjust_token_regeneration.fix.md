Adjusts GcpAuthenticator token regeneration to reflect recent metadata server behaviour changes.

metadata-server (0.4.292 and above) will return a cached token during the last 300-60 seconds of its lifetime (rather than the currently documented behaviour of returning a fresh token during the last 300 seconds).

If a request for a fresh token is made to the metadata server during that window:

- it will return a cached token
- it will also trigger a background refresh process
- if the refresh is successful, the metadata server will update its cache

This change deals with this scenario by retrying the token refresh after 2 seconds if a cached token is determined to have been returned from the metadata server.

authors: garethpelly
