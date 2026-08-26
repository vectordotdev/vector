Fixed two problems with the `chunked_gelf` framing decoder's limits. `pending_messages_limit` was applied to every chunk rather than only to new messages, so once the limit was reached even chunks of messages already pending were rejected and those messages could never complete. Separately, dropping a message for exceeding `max_length` left its timeout task running, so the number of live tasks was not bounded by `pending_messages_limit` the way the pending message count was.

authors: pront
