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
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.args(["/c", script]);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", script]);
            c.env("PATH", "/usr/local/bin:/usr/bin:/bin");
            c.env_remove("HOME");
            c.env_remove("USER");
            c
        };

        let status = cmd
            .current_dir(&self.working_dir)
            .env("NODE_ENV", "production")
            .status()?;

        if !status.success() {
            anyhow::bail!("Script execution failed");
        }

        Ok(())
    }
} 