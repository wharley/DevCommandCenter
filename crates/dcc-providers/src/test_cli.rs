//! Isolated CLI fixture: no PATH mutations, installed providers or network.
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

pub(crate) struct TestCli {
    root: PathBuf,
    pub binary: String,
}

impl TestCli {
    pub fn new(version_output: &str) -> Self {
        let root = std::env::temp_dir().join(format!("dcc-cli-update-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join("cli");
        fs::write(&executable, include_str!("test_cli.sh")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let cli = Self {
            binary: executable.to_str().unwrap().to_string(),
            root,
        };
        cli.write("version", version_output);
        cli.write("features", "multi_agent_v2 experimental true\n");
        cli
    }

    pub fn write(&self, suffix: &str, contents: &str) {
        fs::write(format!("{}.{suffix}", self.binary), contents).unwrap();
    }

    pub fn log(&self) -> String {
        fs::read_to_string(format!("{}.log", self.binary)).unwrap_or_default()
    }
}

impl Drop for TestCli {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
