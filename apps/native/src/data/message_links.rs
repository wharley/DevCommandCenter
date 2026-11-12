//! The images in what the person wrote, for the team room to show as
//! thumbnails under their words; the words, links included, read as the
//! composer's chat reads them (`room_said`).

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
