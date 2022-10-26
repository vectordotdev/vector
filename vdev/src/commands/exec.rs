use clap::Args;

use crate::app::Application;

/// Execute a command within the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl Cli {
    pub fn exec(&self, app: &Application) {
        let mut command = app.command(&self.args[0]);
        command.args(&self.args[1..]);

        let status = command.status().expect("failed to execute command");
        app.exit(status.code().unwrap());
    }
}
