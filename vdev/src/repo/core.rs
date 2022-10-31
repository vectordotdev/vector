use std::process::Command;

use crate::repo::git::Git;

pub struct Repository {
    pub(crate) path: String,
    pub(crate) git: Git,
}

impl Repository {
    pub fn new(path: String) -> Repository {
        Repository {
            path: path.to_string(),
            git: Git::new(path.to_string()),
        }
    }

    pub fn command<T: AsRef<str>>(&self, program: T) -> Command {
        let mut cmd = Command::new(program.as_ref());
        cmd.current_dir(&self.path);
        cmd
    }
}
