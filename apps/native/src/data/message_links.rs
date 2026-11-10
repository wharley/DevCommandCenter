//! The links in what the person wrote, for the team room to show as chips
//! under their words (the way Slack unfurls them): a pull request reads as
//! `app#924`, any other link as its host and path.

use dcc_core::domain::pr_review_request::parse_pull_request_url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageLink {
    pub url: String,
    pub label: String,
    pub pull_request: bool,
}

/// The images the person attached (`@/path.png`, as the composer writes
/// them, spaces in the name included) and the text without them; another
/// attached file keeps its name in the text.
pub fn split_images(text: &str) -> (String, Vec<String>) {
    use crate::data::prompt::{PromptPart, is_image_path, prompt_parts};
    let mut images: Vec<String> = Vec::new();
    let mut words = String::new();
    for part in prompt_parts(text) {
        match part {
            PromptPart::Attachment(path) if path.starts_with('/') && is_image_path(&path) => {
                if !images.contains(&path) {
                    images.push(path);
                }
            }
            PromptPart::Attachment(path) => {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                words.push_str(&name);
            }
            PromptPart::Mention(name) => {
                words.push('@');
                words.push_str(&name);
            }
            PromptPart::Text(piece) => words.push_str(&piece),
        }
    }
    let lines: Vec<String> = words
        .lines()
        .map(|line| {
            line.split(' ')
                .filter(|word| !word.is_empty() && *word != ",")
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    (lines.join("\n"), images)
}

/// The text without its links, and the links in order (each once).
pub fn split_links(text: &str) -> (String, Vec<MessageLink>) {
    let mut words: Vec<&str> = Vec::new();
    let mut links: Vec<MessageLink> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        words.clear();
        for word in line.split(' ') {
            let url = word.trim_end_matches(['.', ',', ';', ')', '!', '?']);
            if url.starts_with("https://") || url.starts_with("http://") {
                if !links.iter().any(|link| link.url == url) {
                    links.push(link_of(url));
                }
            } else {
                words.push(word);
            }
        }
        let kept = words.join(" ");
        if !kept.trim().is_empty() {
            lines.push(kept.trim_end().to_string());
        }
    }
    (lines.join("\n"), links)
}

fn link_of(url: &str) -> MessageLink {
    if let Some((_, repository, number)) = parse_pull_request_url(url) {
        let name = repository.rsplit('/').next().unwrap_or(&repository);
        return MessageLink {
            url: url.to_string(),
            label: format!("{name}#{number}"),
            pull_request: true,
        };
    }
    let bare = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let mut label: String = bare.chars().take(48).collect();
    if bare.chars().count() > 48 {
        label.push('…');
    }
    MessageLink {
        url: url.to_string(),
        label,
        pull_request: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_requests_become_chips_and_the_words_stay() {
        let (text, links) = split_links(
            "preciso que voce revise os PR's https://github.com/VoaHoteis/vendeagora-app/pull/924\nhttps://github.com/VoaHoteis/vendeagora-app/pull/923",
        );
        assert_eq!(text, "preciso que voce revise os PR's");
        let labels: Vec<&str> = links.iter().map(|link| link.label.as_str()).collect();
        assert_eq!(labels, ["vendeagora-app#924", "vendeagora-app#923"]);
        assert!(links.iter().all(|link| link.pull_request));
    }

    #[test]
    fn a_name_with_spaces_is_one_image() {
        let (text, images) = split_images(
            "não funcionou @/Users/me/CleanShot/media_z/CleanShot 2026-11-10 at 12.06.37.png , @/Users/me/CleanShot/media_g/CleanShot 2026-11-10 at 12.07.11.png se olhar vai ver",
        );
        assert_eq!(text, "não funcionou se olhar vai ver");
        assert_eq!(
            images,
            [
                "/Users/me/CleanShot/media_z/CleanShot 2026-11-10 at 12.06.37.png",
                "/Users/me/CleanShot/media_g/CleanShot 2026-11-10 at 12.07.11.png"
            ]
        );
    }

    #[test]
    fn attached_images_leave_the_words() {
        let (text, images) =
            split_images("olha esse erro @/tmp/dcc_img_1.png\n@/tmp/dcc_img_2.jpg e corrige");
        assert_eq!(text, "olha esse erro\ne corrige");
        assert_eq!(images, ["/tmp/dcc_img_1.png", "/tmp/dcc_img_2.jpg"]);
        assert_eq!(split_images("@Suporte oi").1, Vec::<String>::new());
    }

    #[test]
    fn other_links_read_as_host_and_path() {
        let (text, links) = split_links("olha https://developer.apple.com/apple-pay/. e me diz");
        assert_eq!(text, "olha e me diz");
        assert_eq!(links[0].label, "developer.apple.com/apple-pay");
        assert!(!links[0].pull_request);
        assert_eq!(split_links("sem links").1, Vec::new());
    }
}
