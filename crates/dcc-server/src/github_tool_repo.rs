//! Which repository and account a `dcc_github` read uses: `owner/name`, a DCC
//! project's name or id, or the room's project when left out — and, for a
//! repository DCC knows, the account the Pull Requests hub uses for it
//! (`forge::context`); otherwise the account chosen for github.com.

use std::path::Path;
use std::time::Duration;

use dcc_core::domain::forge::ForgeCliProvider;
use dcc_infra::db::SqliteWorkspaceRepo;
use dcc_infra::forge::context::resolve_workspace_forge_context;
use dcc_infra::forge::github_read::RepoSlug;
use dcc_infra::forge::github_read_run::GithubTarget;
use dcc_infra::forge::remote::parse_remote;
use rusqlite::{Connection, OpenFlags};

const DEFAULT_HOST: &str = "github.com";

/// A repository of a DCC project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRepo {
    pub project_id: String,
    pub name: String,
    pub display_name: String,
    pub root_path: String,
    pub remote_url: String,
    pub forge_login: String,
}

impl KnownRepo {
    fn slug(&self) -> Option<(String, RepoSlug)> {
        let remote = parse_remote(&self.remote_url)?;
        let slug = RepoSlug::parse(&format!("{}/{}", remote.namespace, remote.repo))?;
        Some((remote.host, slug))
    }

    fn label(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Known(KnownRepo),
    Slug(RepoSlug),
}

fn read_repos(db_path: &Path) -> Result<Vec<KnownRepo>, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_millis(300))
        .map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare(
            "SELECT project_id, name, COALESCE(display_name, ''), root_path,
                    COALESCE(remote_url, ''), COALESCE(forge_login, '')
             FROM dcc_repositories WHERE project_id NOT LIKE 'dcc-agent-%'
             ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let repos = statement
        .query_map([], |row| {
            Ok(KnownRepo {
                project_id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                root_path: row.get(3)?,
                remote_url: row.get(4)?,
                forge_login: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(repos)
}

/// The repository `repo` names (or the room's, when left out).
pub fn pick(repos: &[KnownRepo], repo: Option<&str>, room: Option<&str>) -> Result<Pick, String> {
    let with_remote = |candidate: &&KnownRepo| candidate.slug().is_some();
    let Some(asked) = repo.map(str::trim).filter(|asked| !asked.is_empty()) else {
        return room
            .and_then(|room| {
                repos
                    .iter()
                    .filter(|known| known.project_id == room)
                    .find(with_remote)
            })
            .cloned()
            .map(Pick::Known)
            .ok_or_else(|| {
                "Pass `repo` as owner/name or a DCC project's name; this room has no GitHub repository.".to_string()
            });
    };
    if let Some(slug) = RepoSlug::parse(asked) {
        let known = repos.iter().find(|known| {
            known.slug().is_some_and(|(_, known)| {
                known.owner.eq_ignore_ascii_case(&slug.owner)
                    && known.name.eq_ignore_ascii_case(&slug.name)
            })
        });
        return Ok(known.cloned().map(Pick::Known).unwrap_or(Pick::Slug(slug)));
    }
    repos
        .iter()
        .filter(|known| {
            known.project_id == asked
                || known.name.eq_ignore_ascii_case(asked)
                || known.display_name.eq_ignore_ascii_case(asked)
        })
        .find(with_remote)
        .cloned()
        .map(Pick::Known)
        .ok_or_else(|| {
            let names: Vec<&str> = repos
                .iter()
                .filter(with_remote)
                .map(KnownRepo::label)
                .collect();
            format!(
                "No DCC project with a GitHub remote is called {asked:?}. Pass owner/name, or one of: {}.",
                names.join(", ")
            )
        })
}

fn is_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// The account DCC chose for github.com, if any (the CLI's active one else).
fn chosen_login(db_path: &Path, host: &str) -> Option<String> {
    SqliteWorkspaceRepo::open(db_path)
        .ok()?
        .get_forge_login_preference("github", host)
        .ok()
        .flatten()
}

fn known_target(db_path: &Path, known: &KnownRepo) -> Result<GithubTarget, String> {
    let bound = Some(known.forge_login.as_str()).filter(|login| !login.is_empty());
    // The hub's own resolution: the remote it reads and the account it uses.
    if let Ok(Some(context)) = resolve_workspace_forge_context(db_path, &known.root_path, bound) {
        if context.provider != ForgeCliProvider::Github {
            return Err(format!("{} is not on GitHub.", known.label()));
        }
        if let Some(repo) = RepoSlug::parse(&format!("{}/{}", context.namespace, context.repo)) {
            return Ok(GithubTarget {
                host: context.host,
                repo,
                login: context.effective_login,
            });
        }
    }
    let (host, repo) = known
        .slug()
        .ok_or_else(|| format!("{} has no GitHub remote.", known.label()))?;
    if host.contains("gitlab") {
        return Err(format!("{} is not on GitHub.", known.label()));
    }
    let login = bound
        .map(str::to_string)
        .or_else(|| chosen_login(db_path, &host));
    Ok(GithubTarget { host, repo, login })
}

pub fn resolve_target(
    db_path: &Path,
    repo: Option<&str>,
    room: Option<&str>,
) -> Result<GithubTarget, String> {
    let repos = read_repos(db_path).unwrap_or_default();
    let target = match pick(&repos, repo, room)? {
        Pick::Known(known) => known_target(db_path, &known)?,
        Pick::Slug(repo) => GithubTarget {
            host: DEFAULT_HOST.to_string(),
            repo,
            login: chosen_login(db_path, DEFAULT_HOST),
        },
    };
    if !is_host(&target.host) {
        return Err(format!("Unexpected GitHub host {:?}.", target.host));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(project: &str, name: &str, remote: &str) -> KnownRepo {
        KnownRepo {
            project_id: project.into(),
            name: name.into(),
            remote_url: remote.into(),
            ..KnownRepo::default()
        }
    }

    #[test]
    fn a_repository_comes_from_the_slug_the_project_or_the_room() {
        let repos = vec![
            known("p1", "Checkout", "git@github.com:Acme/checkout.git"),
            known("p2", "Notes", ""),
            known("p3", "Site", "https://github.com/acme/site"),
        ];
        let slug = |pick: Pick| match pick {
            Pick::Known(known) => known.slug().unwrap().1.full(),
            Pick::Slug(slug) => format!("unknown {}", slug.full()),
        };
        assert_eq!(
            slug(pick(&repos, Some("acme/checkout"), None).unwrap()),
            "Acme/checkout"
        );
        assert_eq!(
            slug(pick(&repos, Some("other/x"), None).unwrap()),
            "unknown other/x"
        );
        assert_eq!(slug(pick(&repos, Some("site"), None).unwrap()), "acme/site");
        assert_eq!(
            slug(pick(&repos, Some("p1"), None).unwrap()),
            "Acme/checkout"
        );
        assert_eq!(slug(pick(&repos, None, Some("p3")).unwrap()), "acme/site");
        assert!(pick(&repos, None, Some("p2")).is_err(), "no remote");
        assert!(pick(&repos, None, None).is_err());
        let error = pick(&repos, Some("Nope"), None).unwrap_err();
        assert!(error.contains("Checkout") && !error.contains("Notes"));
        assert!(is_host("github.acme.com") && !is_host("a b"));
    }
}
