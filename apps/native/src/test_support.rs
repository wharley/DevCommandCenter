//! Helpers the screens' headless tests share.

/// A desktop that answers one request with `response`; joining it gives
/// the request line ("POST /api/v1/…") and the body.
pub fn fake_desktop(response: String) -> (u16, std::thread::JoinHandle<(String, String)>) {
    let (port, server) = fake_desktop_answers(vec![(200, response)]);
    (
        port,
        std::thread::spawn(move || server.join().unwrap().remove(0)),
    )
}

/// A desktop that answers each request in turn with `(status, body)`.
pub fn fake_desktop_answers(
    answers: Vec<(u16, String)>,
) -> (u16, std::thread::JoinHandle<Vec<(String, String)>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        answers
            .into_iter()
            .map(|(status, response)| {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                let request = request.split(' ').take(2).collect::<Vec<_>>().join(" ");
                (request, String::from_utf8(body).unwrap())
            })
            .collect()
    });
    (port, server)
}

/// A desktop that answers by route: each request takes the first answer left
/// for its path, in order; any other path gets a 404 and is not recorded
/// (views also read things the test doesn't follow). It stops once every
/// answer is taken, giving back the recorded requests.
/// Drives the app (`drive`, usually `cx.run_until_parked()`) until the fake
/// desktop got every call it waits for, at most 10 s: a call that never
/// comes fails the test instead of hanging the suite on `join`.
pub fn desktop_done<T>(desktop: &std::thread::JoinHandle<T>, mut drive: impl FnMut()) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        drive();
        if desktop.is_finished() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub fn fake_desktop_routes(
    routes: Vec<(&'static str, u16, String)>,
) -> (u16, std::thread::JoinHandle<Vec<(String, String)>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut routes: Vec<Option<(&'static str, u16, String)>> =
            routes.into_iter().map(Some).collect();
        let mut seen = Vec::new();
        while routes.iter().any(Option::is_some) {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let path = request.split(' ').nth(1).unwrap_or_default().to_string();
            let taken = routes
                .iter_mut()
                .find(|route| route.as_ref().is_some_and(|(route, _, _)| *route == path))
                .and_then(Option::take);
            let (status, response) = match taken {
                Some((_, status, response)) => {
                    let request = request.split(' ').take(2).collect::<Vec<_>>().join(" ");
                    seen.push((request, String::from_utf8(body).unwrap()));
                    (status, response)
                }
                None => (404, "{}".to_string()),
            };
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
        }
        seen
    });
    (port, server)
}

/// Runs git in `dir` (with a fixed author) and fails the test when it does.
pub fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A checkout on `main` with `files` committed (`(path, contents)`).
pub fn git_checkout(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    for (path, contents) in files {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    git(dir.path(), &["add", "-A"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "first"],
    );
    dir
}

/// A database of its own holding one task at `root` (in `worktree` when it
/// has one), for views that read the task by its folder.
pub fn task_database(
    dir: &std::path::Path,
    id: &str,
    root: &std::path::Path,
    worktree: Option<&std::path::Path>,
) -> std::path::PathBuf {
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::workspace::{Workspace, WorkspaceId, WorkspaceState};
    let path = dir.join("database.sqlite");
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&path).unwrap();
    futures::executor::block_on(dcc_core::ports::WorkspaceRepo::save_workspace(
        &repo,
        &Workspace {
            id: WorkspaceId(id.to_string()),
            project_id: ProjectId("project".to_string()),
            name: None,
            root_path: root.to_string_lossy().into_owned(),
            base_branch: "main".to_string(),
            worktree_path: worktree.map(|path| path.to_string_lossy().into_owned()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            snoozed_until: None,
            created_at: "2026-10-24T00:00:00Z".to_string(),
            updated_at: "2026-10-24T00:00:00Z".to_string(),
        },
    ))
    .unwrap();
    path
}
