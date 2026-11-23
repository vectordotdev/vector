Sinks using the shared batcher and request builders now report a failed end-to-end
acknowledgement for routing or encoding failures where events could previously
be acknowledged as delivered before a request was sent.

authors: pront
