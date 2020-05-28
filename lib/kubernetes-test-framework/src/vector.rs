use super::{resource_file::ResourceFile, Result};
use std::process::{Command, Stdio};

/// Takes care of deploying vector into the kubernetes cluster.
///
/// Manages the config file secret accordingly.
#[derive(Debug)]
pub struct Manager {
    interface_command: String,
    namespace: String,
    custom_resource_file: ResourceFile,
}

impl Manager {
    /// Create a new [`Manager`].
    pub fn new(interface_command: &str, namespace: &str, custom_resource: &str) -> Result<Self> {
        let custom_resource_file = ResourceFile::new(custom_resource)?;
        Ok(Self {
            interface_command: interface_command.to_owned(),
            namespace: namespace.to_owned(),
            custom_resource_file,
        })
    }

    pub fn up(&self) -> Result<()> {
        self.exec("up")?;
        Ok(())
    }

    pub fn down(&self) -> Result<()> {
        self.exec("down")?;
        Ok(())
    }

    fn exec(&self, operation: &str) -> Result<()> {
        if !Command::new(&self.interface_command)
            .arg(operation)
            .arg(&self.namespace)
            .env(
                "CUSTOM_RESOURCE_CONIFGS_FILE",
                self.custom_resource_file.path(),
            )
            .stdin(Stdio::null())
            .spawn()?
            .wait()?
            .success()
        {
            Err(format!("failed to exec: {}", operation))?;
        }
        Ok(())
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.down().expect("vector turndown failed");
    }
}
