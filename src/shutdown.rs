use stream_cancel::{Trigger, Tripwire};

// Passed to each Source to coordinate the global shutdown process.
pub struct ShutdownSignals {
    // This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    // its shutdown process.
    pub begin_shutdown: Tripwire,

    // Triggered by an individual Source to inform the global shutdown coordinator that its local
    // shutdown process is complete.
    pub shutdown_complete: Trigger,
}
