//! The login shell's PATH and tool managers' variables for a GUI app (the
//! Finder and the Dock start it with launchd's minimal PATH, without nvm,
//! fnm, volta or Homebrew). Call once, first thing, before other threads.

pub fn bootstrap() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let (tx, rx) = std::sync::mpsc::channel();
    let shell_clone = shell.clone();
    std::thread::spawn(move || {
        let result = std::process::Command::new(&shell_clone)
            .args(["-ilc", "env"])
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(out)) => out,
        _ => {
            eprintln!("[DCC] bootstrap_shell_env: timeout ou erro ao capturar env do shell");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut new_path: Option<String> = None;
    let keys_to_apply = [
        "PATH",
        "NVM_DIR",
        "NVM_BIN",
        "FNM_DIR",
        "VOLTA_HOME",
        "PNPM_HOME",
    ];

    for line in stdout.lines() {
        if let Some(idx) = line.find('=') {
            let key = &line[..idx];
            let val = &line[idx + 1..];
            if key == "PATH" {
                new_path = Some(val.to_string());
            } else if keys_to_apply.contains(&key) {
                std::env::set_var(key, val);
            }
        }
    }

    // PATH sempre é substituído pelo do shell para garantir acesso a node/yarn/etc.
    if let Some(path) = new_path {
        if !path.is_empty() {
            std::env::set_var("PATH", &path);
            eprintln!(
                "[DCC] PATH atualizado via shell de login: {}",
                &path[..path.len().min(120)]
            );
        }
    }
}
