mod compilation;
pub use compilation::WasmCompilation;

mod hostcall;
pub use hostcall::Hostcall;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum State {
    Beginning,
    Completed,
}

impl State {
    /// Cheaply turn into a `&'static str` so you don't need to format it for metrics.
    pub fn as_const_str(&self) -> &'static str {
        match self {
            State::Beginning => BEGINNING,
            State::Completed => COMPLETED,
        }
    }
}

const BEGINNING: &str = "beginning";
const COMPLETED: &str = "completed";