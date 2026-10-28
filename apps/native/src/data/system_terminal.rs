//! `shell_open_terminal_at_path`: the system's terminal in a folder.

use std::path::Path;
use std::process::Command;

pub fn open(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("Diretório inválido: {}", path.display()));
    }
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .status()
            .map_err(|error| format!("Falha ao abrir Terminal.app: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("Terminal.app retornou erro ao abrir o diretório".into())
        }
    }
    #[cfg(target_os = "linux")]
    {
        for (bin, flag) in [
            ("x-terminal-emulator", "--working-directory"),
            ("gnome-terminal", "--working-directory"),
            ("konsole", "--workdir"),
            ("xfce4-terminal", "--working-directory"),
        ] {
            if Command::new(bin)
                .arg(flag)
                .arg(path)
                .status()
                .is_ok_and(|status| status.success())
            {
                return Ok(());
            }
        }
        Err("Nenhum terminal encontrado".into())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(path)
            .status()
            .map(drop)
            .map_err(|error| error.to_string())
    }
}
