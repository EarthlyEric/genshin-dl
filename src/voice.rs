use std::path::Path;

use anyhow::bail;

use crate::edition::Edition;

/// Collect all voiceover `matching_field` values from the given manifest fields.
/// The `game` field itself is always excluded.
pub fn voice_fields<'a>(fields: impl Iterator<Item = &'a str>) -> Vec<String> {
    fields
        .filter(|field| *field != "game")
        .map(str::to_owned)
        .collect()
}

/// Resolve the requested voiceover fields against the available ones.
///
/// - empty request + `default_all` => all available voices
/// - empty request + !`default_all` => no voices
/// - `all` => all available voices
/// - `none` => no voices
/// - otherwise validate every requested field against the available list
pub fn resolve(
    requested: &[String],
    available: &[String],
    default_all: bool,
) -> anyhow::Result<Vec<String>> {
    if requested.iter().any(|r| r == "none") {
        return Ok(vec![]);
    }

    if requested.is_empty() {
        return Ok(if default_all {
            available.to_vec()
        } else {
            vec![]
        });
    }

    if requested.iter().any(|r| r == "all") {
        return Ok(available.to_vec());
    }

    for r in requested {
        if !available.iter().any(|a| a == r) {
            bail!(
                "unknown voiceover package '{r}', available: {}",
                available.join(", ")
            );
        }
    }

    Ok(requested.to_vec())
}

/// Map a voiceover folder name to its locale code, as used by the Sophon
/// manifest/diff `matching_field` values.
fn folder_to_code(folder: &str) -> Option<&'static str> {
    match folder {
        "English(US)" | "English" => Some("en-us"),
        "Japanese" => Some("ja-jp"),
        "Korean" => Some("ko-kr"),
        "Chinese" | "Chinese(PRC)" => Some("zh-cn"),
        _ => None,
    }
}

/// Detect already installed voiceover packages by scanning the game's
/// `AudioAssets` folder, returning locale codes.
pub fn detect_installed(game_dir: &Path, edition: Edition) -> Vec<String> {
    let base = game_dir
        .join(edition.data_folder())
        .join("StreamingAssets/AudioAssets");

    let Ok(entries) = std::fs::read_dir(base) else {
        return vec![];
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| folder_to_code(&name).map(str::to_owned))
        .collect()
}

/// Split a comma separated voiceover argument list
pub fn parse_arg(s: Option<&str>) -> Vec<String> {
    s.map(|v| {
        v.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_empty() {
        let available = vec!["en-us".to_string(), "ja-jp".to_string()];
        assert_eq!(resolve(&[], &available, true).unwrap(), available);
        assert!(resolve(&[], &available, false).unwrap().is_empty());
    }

    #[test]
    fn resolve_none() {
        let available = vec!["en-us".to_string(), "ja-jp".to_string()];
        assert!(resolve(&["none".to_string()], &available, true)
            .unwrap()
            .is_empty());
        assert!(resolve(&["none".to_string()], &available, false)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn resolve_all() {
        let available = vec!["en-us".to_string(), "ja-jp".to_string()];
        assert_eq!(
            resolve(&["all".to_string()], &available, false).unwrap(),
            available
        );
    }

    #[test]
    fn resolve_selected() {
        let available = vec!["en-us".to_string(), "ja-jp".to_string()];
        assert_eq!(
            resolve(&["en-us".to_string()], &available, false).unwrap(),
            vec!["en-us".to_string()]
        );
    }

    #[test]
    fn resolve_unknown() {
        let available = vec!["en-us".to_string()];
        assert!(resolve(&["xx".to_string()], &available, false).is_err());
    }

    #[test]
    fn voice_fields_skips_game() {
        let fields = vec!["game", "en-us", "ja-jp"];
        assert_eq!(voice_fields(fields.into_iter()), vec!["en-us", "ja-jp"]);
    }

    #[test]
    fn folder_names_map_to_locale_codes() {
        assert_eq!(folder_to_code("English(US)"), Some("en-us"));
        assert_eq!(folder_to_code("English"), Some("en-us"));
        assert_eq!(folder_to_code("Japanese"), Some("ja-jp"));
        assert_eq!(folder_to_code("Korean"), Some("ko-kr"));
        assert_eq!(folder_to_code("Chinese"), Some("zh-cn"));
        assert_eq!(folder_to_code("Chinese(PRC)"), Some("zh-cn"));
        assert_eq!(folder_to_code("French"), None);
    }

    #[test]
    fn detect_installed_returns_locale_codes() {
        let dir = std::env::temp_dir().join(format!("genshin-dl-test-{}", std::process::id()));
        let audio = dir
            .join(Edition::Global.data_folder())
            .join("StreamingAssets/AudioAssets");
        std::fs::create_dir_all(audio.join("English(US)")).unwrap();
        std::fs::create_dir_all(audio.join("Japanese")).unwrap();
        std::fs::create_dir_all(audio.join("unrelated")).unwrap();

        let mut detected = detect_installed(&dir, Edition::Global);
        detected.sort();
        assert_eq!(detected, vec!["en-us", "ja-jp"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
