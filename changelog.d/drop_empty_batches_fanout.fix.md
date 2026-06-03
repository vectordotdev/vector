Fixed a crash that could occur when a source or transform emitted an empty event batch into a topology with downstream buffers. Vector now
drops empty batches before they reach those buffers and logs a warning identifying the upstream component.

authors: graphcareful
