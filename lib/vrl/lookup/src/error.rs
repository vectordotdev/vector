use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum LookupError {
    #[snafu(display("Invalid path: {}.", message))]
    Invalid { message: String },
}
