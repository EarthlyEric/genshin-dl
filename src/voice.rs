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
/// - otherwise validate every requested field against the available list
pub fn resolve(
    requested: &[String],
    available: &[String],
    default_all: bool,
) -> anyhow::Result<Vec<String>> {
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

/// Detect already installed voiceover packages by scanning the game's
/// `AudioAssets` folder.
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
}
