The `mqtt` source now subscribes to its topics again after reconnecting when the broker reports that the session is no longer present (for example after a broker restart, an expired session, or a deleted subscription queue). Previously the source stayed connected but silently stopped receiving messages until Vector was restarted.

authors: florissmit10
