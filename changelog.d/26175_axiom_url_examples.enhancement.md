The `axiom` sink's `url` option shows example values in the documentation again, and the generated advanced example configuration now demonstrates `url` instead of `region`. Previously the examples had to be stripped from `url` because the example-config generator emitted both `url` and `region`, and setting both is rejected by the sink's validation.

authors: Ash20pk
