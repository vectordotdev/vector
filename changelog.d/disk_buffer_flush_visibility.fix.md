Fix disk-buffered events occasionally stalling until another write or a restart by retrying transient EOF when published writer progress indicates unread data. Idle readers continue to wait for notifications.

authors: phihos
