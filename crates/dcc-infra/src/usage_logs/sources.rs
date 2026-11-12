//! Where the logs are and what DCC knows about them: the Claude and Codex
//! homes (the system ones and every account DCC manages), and from DCC's
//! database the Claude sessions it started and the paths it runs in, so
//! a Claude SDK session there reads as "via DCC" (Codex says so itself:
//! DCC's app-server client is named `dcc`, the rollout's `originator`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use dcc_core::domain::usage_logs::Via;

/// The directories to scan.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    /// `<claude config>/projects`
    pub claude: Vec<PathBuf>,
    /// `<codex home>/sessions` and `<codex home>/archived_sessions`
    pub codex: Vec<PathBuf>,
}

/// What DCC's database says about the sessions it ran.
#[derive(Clone, Debug, Default)]
pub struct DccHints {
    /// Claude session ids DCC resumed (`dcc_provider_native_sessions`).
    pub claude_sessions: HashSet<String>,
    /// Project folders, worktrees and DCC's own data folder.
    pub paths: Vec<String>,
    /// DCC worktree path → its project's folder name (a worktree sits in
    /// `.dcc-worktrees/<branch>`, which does not name the repository).
    pub worktrees: Vec<(String, String)>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

fn push_unique(list: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() && !list.contains(&path) {
        list.push(path);
    }
}

fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// The system homes, the accounts in DCC's database (`dcc_db`, opened
/// read-only; missing or unreadable means none) and its hints.
pub fn discover(dcc_db: Option<&Path>) -> (Sources, DccHints) {
    let mut sources = Sources::default();
    let mut hints = DccHints::default();
    let claude_home = env_dir("CLAUDE_CONFIG_DIR").or_else(|| home().map(|h| h.join(".claude")));
    let codex_home = env_dir("CODEX_HOME").or_else(|| home().map(|h| h.join(".codex")));
    let mut claude_homes: Vec<PathBuf> = claude_home.into_iter().collect();
    let mut codex_homes: Vec<PathBuf> = codex_home.into_iter().collect();

    if let Some(db) = dcc_db.filter(|db| db.is_file()) {
        if let Some(data_dir) = db.parent() {
            hints.paths.push(data_dir.to_string_lossy().into_owned());
        }
        if let Ok(conn) =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        {
            let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
            read_hints(&conn, &mut hints, &mut claude_homes, &mut codex_homes);
        }
    }
    for dir in claude_homes {
        push_unique(&mut sources.claude, dir.join("projects"));
    }
    for dir in codex_homes {
        push_unique(&mut sources.codex, dir.join("sessions"));
        push_unique(&mut sources.codex, dir.join("archived_sessions"));
    }
    (sources, hints)
}

fn strings(conn: &rusqlite::Connection, sql: &str) -> Vec<(String, String)> {
    let Ok(mut statement) = conn.prepare(sql) else {
        return Vec::new();
    };
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

fn read_hints(
    conn: &rusqlite::Connection,
    hints: &mut DccHints,
    claude_homes: &mut Vec<PathBuf>,
    codex_homes: &mut Vec<PathBuf>,
) {
    for (provider, dir) in strings(
        conn,
        "SELECT provider_id, config_dir FROM dcc_provider_accounts",
    ) {
        match provider.as_str() {
            "claude_code" => claude_homes.push(PathBuf::from(dir)),
            "codex" => codex_homes.push(PathBuf::from(dir)),
            _ => {}
        }
    }
    hints.claude_sessions = strings(
        conn,
        "SELECT native_id, '' FROM dcc_provider_native_sessions WHERE provider_id = 'claude_code'",
    )
    .into_iter()
    .map(|(id, _)| id)
    .collect();
    for (path, _) in strings(conn, "SELECT path, '' FROM projects") {
        hints.paths.push(path);
    }
    for (worktree, project_path) in strings(
        conn,
        "SELECT w.worktree_path, COALESCE(p.path, w.root_path) FROM dcc_workspaces w \
         LEFT JOIN projects p ON p.id = w.project_id",
    ) {
        if !worktree.is_empty() {
            hints.paths.push(worktree.clone());
            hints.worktrees.push((worktree, folder_name(&project_path)));
        }
    }
    for (root, _) in strings(conn, "SELECT root_path, '' FROM dcc_workspaces") {
        hints.paths.push(root);
    }
    hints.paths.retain(|path| !path.is_empty());
    hints.paths.sort();
    hints.paths.dedup();
}

/// Names projects and tells DCC sessions apart, remembering each folder.
#[derive(Debug, Default)]
pub struct Resolver {
    pub hints: DccHints,
    projects: HashMap<String, String>,
}

fn under(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('\\'))
}

impl Resolver {
    pub fn new(hints: DccHints) -> Self {
        Self {
            hints,
            projects: HashMap::new(),
        }
    }

    fn in_dcc(&self, cwd: &str) -> bool {
        cwd.contains("/.dcc-worktrees/") || self.hints.paths.iter().any(|root| under(cwd, root))
    }

    /// The project a working directory belongs to: a DCC worktree's
    /// project, else the git repository's folder (a linked worktree's main
    /// repository), else the folder itself.
    pub fn project(&mut self, cwd: &str) -> String {
        if cwd.is_empty() {
            return String::new();
        }
        if let Some(name) = self.projects.get(cwd) {
            return name.clone();
        }
        let name = self
            .hints
            .worktrees
            .iter()
            .find(|(worktree, _)| under(cwd, worktree))
            .map(|(_, name)| name.clone())
            .or_else(|| git_root_name(Path::new(cwd)))
            .unwrap_or_else(|| folder_name(cwd));
        self.projects.insert(cwd.to_string(), name.clone());
        name
    }

    /// Claude's `entrypoint`; an SDK session DCC knows, or one in DCC's
    /// folders, is DCC's (DCC runs Claude through the Agent SDK).
    pub fn claude_via(&self, entrypoint: &str, session: &str, cwd: &str) -> Via {
        if self.hints.claude_sessions.contains(session) {
            return Via::Dcc;
        }
        match entrypoint {
            "cli" => Via::Cli,
            "claude-desktop" | "desktop" => Via::Desktop,
            "" => Via::Other,
            entry if entry.starts_with("sdk") => {
                if self.in_dcc(cwd) {
                    Via::Dcc
                } else {
                    Via::Sdk
                }
            }
            entry
                if entry.contains("vscode")
                    || entry.contains("jetbrains")
                    || entry.contains("ide") =>
            {
                Via::Ide
            }
            _ => Via::Other,
        }
    }
}

/// Codex's `originator`, else its `source`.
pub fn codex_via(originator: &str, source: &str) -> Via {
    let originator = originator.to_ascii_lowercase();
    if originator.starts_with("dcc") {
        return Via::Dcc;
    }
    match originator.as_str() {
        "codex-tui" | "codex_tui" => return Via::Tui,
        "codex_exec" | "codex-exec" => return Via::Exec,
        "codex_vscode" | "codex-vscode" => return Via::Desktop,
        "codex_cli_rs" | "codex-cli" | "codex_cli" => return Via::Cli,
        name if name.contains("desktop") => return Via::Desktop,
        _ => {}
    }
    match source {
        "cli" => Via::Cli,
        "exec" => Via::Exec,
        "vscode" => Via::Desktop,
        _ => Via::Other,
    }
}

/// The repository folder of `dir`: the nearest folder with `.git`; for a
/// linked worktree (`.git` is a file naming `<repo>/.git/worktrees/x`),
/// the main repository's folder.
fn git_root_name(dir: &Path) -> Option<String> {
    let home = home();
    let mut current = Some(dir);
    while let Some(folder) = current {
        if home.as_deref() == Some(folder) {
            return None;
        }
        let git = folder.join(".git");
        if git.is_dir() {
            return folder.file_name().map(|n| n.to_string_lossy().into_owned());
        }
        if git.is_file() {
            let linked = std::fs::read_to_string(&git).ok().and_then(|text| {
                let gitdir = text.trim().strip_prefix("gitdir:")?.trim().to_string();
                let repo = gitdir.split("/.git/").next()?.to_string();
                Some(folder_name(&repo))
            });
            return linked.or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()));
        }
        current = folder.parent();
    }
    None
}
