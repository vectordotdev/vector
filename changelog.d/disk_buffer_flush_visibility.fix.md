Fix disk-buffered events occasionally stalling until another write or a restart by retrying transient EOF when published writer progress indicates unread data. Readers that have consumed all published bytes continue to wait for notifications, including when earlier deliveries remain unacknowledged.

authors: phihos
