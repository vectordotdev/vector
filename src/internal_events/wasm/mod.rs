mod compilation;
pub use compilation::WasmCompilationProgress;

mod hostcall;
pub use hostcall::WasmHostcallProgress;

mod event_processing;
pub use event_processing::EventProcessingProgress;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum State {
    Beginning,
    Cached,
    Completed,
    Errored,
}

impl State {
    /// Cheaply turn into a `&'static str` so you don't need to format it for metrics.
    pub fn as_const_str(self) -> &'static str {
        match self {
            State::Beginning => BEGINNING,
            State::Completed => COMPLETED,
            State::Errored => ERRORED,
            State::Cached => CACHED,
        }
    }
}

const BEGINNING: &str = "beginning";
const COMPLETED: &str = "completed";
const CACHED: &str = "cached";
const ERRORED: &str = "errored";
