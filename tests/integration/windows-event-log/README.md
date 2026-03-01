# Windows Event Log integration tests

These tests exercise the Windows Event Log source against the local Windows Event Log service.
There is no docker-compose environment because the Event Log service is provided by the host OS.

The tests use `eventcreate` to write a test event to the Application log. This may require
administrator privileges on some systems.

Run on Windows:
- `make test-integration-windows-event-log`
- `cargo test -p vector --no-default-features --features sources-windows_event_log-integration-tests windows_event_log::integration_tests`
