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

    /// Run a Node.js script directly using `node`, bypassing npm or shell invocation.
    ///
    /// `node_args` is everything after `"node "` in the postinstall script string.
    /// For example, if the script is `"node scripts/postinstall.js"`, pass
    /// `"scripts/postinstall.js"`. Only pure `node <path>` invocations are executed;
    /// scripts that invoke external tools (husky, cmake, gyp, etc.) must be filtered
    /// by the caller before reaching this method.
    pub async fn run_node_script(&self, node_args: &str) -> Result<()> {
        let args: Vec<&str> = node_args.split_whitespace().collect();
        let status = Command::new("node")
            .args(&args)
            .current_dir(&self.working_dir)
            .env("NODE_ENV", "production")
            .status()?;

        if !status.success() {
            anyhow::bail!("Node script execution failed");
        }

        Ok(())
    }
}
