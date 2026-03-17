Reduced the memory usage in the `aggregate` transform where previous values were being held
even if `mode` was not set to `Diff`. However, this is still an issue if `mode` if `Diff`.

authors: thomasqueirozb
