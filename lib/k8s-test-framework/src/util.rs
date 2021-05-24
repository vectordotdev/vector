use crate::Result;

pub async fn run_command(mut command: tokio::process::Command) -> Result<()> {
    let exit_status = command.spawn()?.wait().await?;
    if !exit_status.success() {
        return Err(format!("exec failed: {:?}", command).into());
    }
    Ok(())
}

pub fn run_command_blocking(mut command: std::process::Command) -> Result<()> {
    let exit_status = command.spawn()?.wait()?;
    if !exit_status.success() {
        return Err(format!("exec failed: {:?}", command).into());
    }
    Ok(())
}

pub async fn run_command_output(mut command: tokio::process::Command) -> Result<String> {
    let output = command.spawn()?.wait_with_output().await?;
    if !output.status.success() {
        return Err(format!("exec failed: {:?}", command).into());
    }

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}
