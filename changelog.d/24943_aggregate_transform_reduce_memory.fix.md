Reduced the memory usage in the `aggregate` transform where previous values were being held
even if `mode` was not set to `Diff`.

authors: thomasqueirozb
