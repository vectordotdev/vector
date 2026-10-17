`vector tap` no longer receives duplicate events after configuration reloads that leave a tapped
component unchanged, and stops tapping outputs that no longer match the requested patterns. Events
dropped because the tap client cannot keep up are now reported in a rate-limited warning log
instead of being dropped silently.

authors: 20agbekodo
