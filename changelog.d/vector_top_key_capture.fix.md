Fixed a startup race in `vector top` on Unix. Depending on thread timing, it could exit with
`The cursor position could not be read within a normal duration` before drawing the dashboard.

authors: thomasqueirozb
