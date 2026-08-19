use std::fmt::Display;
use std::str::FromStr;

use anime_launcher_sdk::anime_game_core::sophon::GameEdition as SophonEdition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    Global,
    China,
}

impl Edition {
    pub const ALL: [Edition; 2] = [Self::Global, Self::China];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::China => "china",
        }
    }

    pub fn game_id(self) -> &'static str {
        match self {
            Self::Global => "gopR6Cufr3",
            Self::China => "1Z8W5NHUQb",
        }
    }

    /// Game data folder, used to locate the voiceover packages path
    pub fn data_folder(self) -> &'static str {
        match self {
            Self::Global => "GenshinImpact_Data",
            Self::China => "YuanShen_Data",
        }
    }

    pub fn sophon(self) -> SophonEdition {
        SophonEdition::from_str(self.as_str()).expect("edition string is always valid")
    }
}

impl FromStr for Edition {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "global" => Ok(Self::Global),
            "china" | "cn" => Ok(Self::China),
            other => Err(anyhow::anyhow!("unknown edition: {other}")),
        }
    }
}

impl Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_editions() {
        assert_eq!(Edition::from_str("global").unwrap(), Edition::Global);
        assert_eq!(Edition::from_str("china").unwrap(), Edition::China);
        assert_eq!(Edition::from_str("CN").unwrap(), Edition::China);
        assert!(Edition::from_str("jp").is_err());
    }

    #[test]
    fn game_ids() {
        assert_eq!(Edition::Global.game_id(), "gopR6Cufr3");
        assert_eq!(Edition::China.game_id(), "1Z8W5NHUQb");
    }

    #[test]
    fn sophon_conversion() {
        assert_eq!(
            Edition::Global.sophon(),
            SophonEdition::from_str("global").unwrap()
        );
    }
}
