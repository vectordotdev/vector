Fixed `azure_blob` sink to use the correct compilation feature. This prevents panics that sometimes
happens when the old `azure_core` (v0.21) is used to make requests instead of the newest (v0.25).

authors: thomasqueirozb
