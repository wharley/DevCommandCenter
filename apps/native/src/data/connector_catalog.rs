//! The curated connectors (`catalog.ts`, data only) and the Slack app the
//! person creates when a server registers no clients (`user-app.ts`).

use serde_json::{Value, json};

/// `USER_APP_REDIRECT_URI`
pub const USER_APP_REDIRECT_URI: &str = "http://localhost:46327/oauth/mcp/callback";

/// `SLACK_WRITE_SCOPES`: sending a message (or a thread reply) as the person.
/// Each send still waits for their approval in the conversation.
pub const SLACK_WRITE_SCOPES: [&str; 1] = ["chat:write"];

/// Every scope the Slack app asks for: the reads, then the writes.
pub fn slack_scopes() -> Vec<&'static str> {
    SLACK_READ_SCOPES
        .iter()
        .chain(SLACK_WRITE_SCOPES.iter())
        .copied()
        .collect()
}

/// `SLACK_READ_SCOPES`
pub const SLACK_READ_SCOPES: [&str; 21] = [
    "search:read.public",
    "search:read.private",
    "search:read.mpim",
    "search:read.im",
    "search:read.files",
    "search:read.users",
    "channels:history",
    "groups:history",
    "mpim:history",
    "im:history",
    "channels:read",
    "groups:read",
    "mpim:read",
    "im:read",
    "users:read",
    "users:read.email",
    "files:read",
    "canvases:read",
    "lists:read",
    "reactions:read",
    "emoji:read",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Popular,
    Developer,
    Productivity,
}

impl Category {
    pub const ALL: [Category; 3] = [
        Category::Popular,
        Category::Developer,
        Category::Productivity,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Popular => "popular",
            Category::Developer => "developer",
            Category::Productivity => "productivity",
        }
    }
}

/// How a preset signs in when the server registers no clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auth {
    OAuth,
    Token,
    SlackApp,
}

/// `ConnectorPreset`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub url: &'static str,
    pub docs_url: &'static str,
    pub logo: Option<&'static str>,
    pub auth: Auth,
    pub token_help_url: Option<&'static str>,
    pub hidden: bool,
}

/// `CONNECTOR_PRESETS`
pub const PRESETS: [Preset; 17] = [
    Preset {
        id: "clickup",
        name: "ClickUp",
        category: Category::Popular,
        url: "https://mcp.clickup.com/mcp",
        docs_url: "https://developer.clickup.com/docs/connect-an-ai-assistant-to-clickups-mcp-server",
        logo: Some("clickup"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "linear",
        name: "Linear",
        category: Category::Popular,
        url: "https://mcp.linear.app/mcp",
        docs_url: "https://linear.app/docs/mcp",
        logo: Some("linear"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "notion",
        name: "Notion",
        category: Category::Popular,
        url: "https://mcp.notion.com/mcp",
        docs_url: "https://developers.notion.com/docs/mcp",
        logo: Some("notion"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "github",
        name: "GitHub",
        category: Category::Developer,
        url: "https://api.githubcopilot.com/mcp/",
        docs_url: "https://github.com/github/github-mcp-server",
        logo: Some("github"),
        auth: Auth::Token,
        token_help_url: Some("https://github.com/settings/personal-access-tokens/new"),
        hidden: false,
    },
    Preset {
        id: "sentry",
        name: "Sentry",
        category: Category::Developer,
        url: "https://mcp.sentry.dev/mcp",
        docs_url: "https://docs.sentry.io/product/sentry-mcp/",
        logo: Some("sentry"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "slack",
        name: "Slack",
        category: Category::Productivity,
        url: "https://mcp.slack.com/mcp",
        docs_url: "https://docs.slack.dev/ai/mcp-server/",
        logo: Some("slack"),
        auth: Auth::SlackApp,
        token_help_url: None,
        hidden: false,
    },
    // Servers that register DCC themselves (checked 2026-11-05 against
    // each one's OAuth metadata), so they connect in one click.
    Preset {
        id: "supabase",
        name: "Supabase",
        category: Category::Developer,
        url: "https://mcp.supabase.com/mcp",
        docs_url: "https://supabase.com/docs/guides/getting-started/mcp",
        logo: Some("supabase"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "vercel",
        name: "Vercel",
        category: Category::Developer,
        url: "https://mcp.vercel.com",
        docs_url: "https://vercel.com/docs/mcp/vercel-mcp",
        logo: Some("vercel"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "cloudflare",
        name: "Cloudflare",
        category: Category::Developer,
        url: "https://bindings.mcp.cloudflare.com/mcp",
        docs_url: "https://developers.cloudflare.com/agents/model-context-protocol/mcp-servers-for-cloudflare/",
        logo: Some("cloudflare"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "neon",
        name: "Neon",
        category: Category::Developer,
        url: "https://mcp.neon.tech/mcp",
        docs_url: "https://neon.com/docs/ai/neon-mcp-server",
        logo: Some("neon"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "netlify",
        name: "Netlify",
        category: Category::Developer,
        url: "https://netlify-mcp.netlify.app/mcp",
        docs_url: "https://docs.netlify.com/build/build-with-ai/netlify-mcp-server/",
        logo: Some("netlify"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "posthog",
        name: "PostHog",
        category: Category::Developer,
        url: "https://mcp.posthog.com/mcp",
        docs_url: "https://posthog.com/docs/model-context-protocol",
        logo: Some("posthog"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "revenuecat",
        name: "RevenueCat",
        category: Category::Developer,
        url: "https://mcp.revenuecat.ai/mcp",
        docs_url: "https://www.revenuecat.com/docs/tools/mcp",
        logo: Some("revenuecat"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "resend",
        name: "Resend",
        category: Category::Developer,
        url: "https://mcp.resend.com/mcp",
        docs_url: "https://resend.com/docs/mcp-server",
        logo: Some("resend"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "atlassian",
        name: "Atlassian",
        category: Category::Productivity,
        url: "https://mcp.atlassian.com/v1/mcp",
        docs_url: "https://support.atlassian.com/atlassian-rovo-mcp-server/docs/getting-started-with-the-atlassian-remote-mcp-server/",
        logo: Some("atlassian"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    Preset {
        id: "trello",
        name: "Trello",
        category: Category::Productivity,
        url: "https://mcp.trello.com/v1",
        docs_url: "https://github.com/atlassian/trello-mcp-server",
        logo: Some("trello"),
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: false,
    },
    // Production answers 403 until the server is deployed.
    Preset {
        id: "zedy",
        name: "Zedy",
        category: Category::Productivity,
        url: "https://app.zedy.com.br/mcp",
        docs_url: "https://app.zedy.com.br/docs",
        logo: None,
        auth: Auth::OAuth,
        token_help_url: None,
        hidden: true,
    },
];

impl Preset {
    /// Scopes asked for at sign-in instead of the server's (Slack's reads
    /// and `chat:write`).
    pub fn scopes(&self) -> Option<String> {
        (self.auth == Auth::SlackApp).then(|| slack_scopes().join(" "))
    }
}

/// `visibleConnectorPresets`
pub fn visible_presets() -> impl Iterator<Item = &'static Preset> {
    PRESETS.iter().filter(|preset| !preset.hidden)
}

/// `sameConnectorUrl`: ignoring a trailing slash, the host's case and a fragment.
pub fn same_url(left: &str, right: &str) -> bool {
    fn normalize(value: &str) -> String {
        let value = value.trim();
        let without_fragment = value.split('#').next().unwrap_or(value);
        let Some((scheme, rest)) = without_fragment.split_once("://") else {
            return value.trim_end_matches('/').to_string();
        };
        let (host, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
        let (path, query) = match path.split_once('?') {
            Some((path, query)) => (path, format!("?{query}")),
            None => (path, String::new()),
        };
        format!(
            "{}://{}{}{query}",
            scheme.to_lowercase(),
            host.to_lowercase(),
            path.trim_end_matches('/')
        )
    }
    normalize(left) == normalize(right)
}

pub fn preset_for_url(url: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| same_url(preset.url, url))
}

/// `slackAppManifest`
pub fn slack_manifest() -> Value {
    json!({
        "display_information": {
            "name": "Dev Command Center",
            "description": "Lets your coding agents in Dev Command Center read Slack and, with your approval, send messages.",
        },
        "oauth_config": {
            "redirect_urls": [USER_APP_REDIRECT_URI],
            "scopes": { "user": slack_scopes() },
            "pkce_enabled": true,
        },
        "settings": {
            "is_mcp_enabled": true,
            "token_rotation_enabled": true,
            "org_deploy_enabled": false,
            "socket_mode_enabled": false,
        },
    })
}

/// `slackCreateAppUrl`: "create app from manifest" with it filled in.
pub fn slack_create_app_url() -> String {
    let manifest = slack_manifest().to_string();
    let mut encoded = String::new();
    for byte in manifest.bytes() {
        // `encodeURIComponent`
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("https://api.slack.com/apps?new_app=1&manifest_json={encoded}")
}
