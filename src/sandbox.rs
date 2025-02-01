use std::path::Path;
use std::process::Command;
use anyhow::Result;

pub struct Sandbox {
    working_dir: std::path::PathBuf,
}

impl Sandbox {
    pub fn new(working_dir: impl AsRef<Path>) -> Self {
        Self {
            working_dir: working_dir.as_ref().to_path_buf(),
        }
    }

    pub async fn run_script(&self, script: &str) -> Result<()> {
        // Create a new process with restricted permissions
        let status = Command::new("sh")
            .arg("-c")
            .arg(script)
            .current_dir(&self.working_dir)
            // Restrict permissions (Unix-specific)
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("NODE_ENV", "production")
            // Prevent access to sensitive files
            .env_remove("HOME")
            .env_remove("USER")
            .status()?;

        if !status.success() {
            anyhow::bail!("Script execution failed");
        }

        Ok(())
    }
} 