The statsd source sanitizes stats keys, replacing '/' with '-', whitespace with '_' and removing all
other non alphanumeric characters.

The enhacements allows disabling this behavior.
