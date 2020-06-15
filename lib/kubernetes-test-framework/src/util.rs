use crate::Result;

pub async fn run_command(mut command: tokio::process::Command) -> Result<()> {
    let exit_status = command.spawn()?.await?;
    if !exit_status.success() {
        Err(format!("exec failed: {:?}", command))?;
    }
    Ok(())
}

pub fn run_command_blocking(mut command: std::process::Command) -> Result<()> {
    let mut child = command.spawn()?;
    let exit_status = child.wait()?;
    if !exit_status.success() {
        Err(format!("exec failed: {:?}", command))?;
    }
    Ok(())
}
