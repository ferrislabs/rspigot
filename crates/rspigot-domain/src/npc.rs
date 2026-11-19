use std::fmt;

pub const MAX_NAME_LEN: usize = 16;
pub const MAX_KEY_LEN: usize = 64;
pub const DEFAULT_SKIN_LAYERS: u8 = 0x7f;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NpcNameError {
    #[error("an npc name has 1 to {MAX_NAME_LEN} characters, got {0}")]
    Length(usize),
    #[error("an npc name holds letters, digits and underscores only, got `{0}`")]
    Character(char),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NpcName(String);

impl NpcName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NpcName {
    type Error = NpcNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let length = name.chars().count();
        if length == 0 || length > MAX_NAME_LEN {
            return Err(NpcNameError::Length(length));
        }
        if let Some(bad) = name
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
        {
            return Err(NpcNameError::Character(bad));
        }
        Ok(Self(name))
    }
}

impl fmt::Display for NpcName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NpcKeyError {
    #[error("an npc key has 1 to {MAX_KEY_LEN} characters, got {0}")]
    Length(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NpcKey(String);

impl NpcKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NpcKey {
    type Error = NpcKeyError;

    fn try_from(key: String) -> Result<Self, Self::Error> {
        let length = key.chars().count();
        if length == 0 || length > MAX_KEY_LEN {
            return Err(NpcKeyError::Length(length));
        }
        Ok(Self(key))
    }
}

impl fmt::Display for NpcKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinTextures {
    pub value: String,
    pub signature: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_holds_one_to_sixteen_word_characters() {
        assert!(NpcName::try_from("Scout_7".to_owned()).is_ok());
        assert!(NpcName::try_from("a".repeat(16)).is_ok());
        assert_eq!(
            NpcName::try_from(String::new()),
            Err(NpcNameError::Length(0))
        );
        assert_eq!(
            NpcName::try_from("a".repeat(17)),
            Err(NpcNameError::Length(17))
        );
        assert_eq!(
            NpcName::try_from("Scout 7".to_owned()),
            Err(NpcNameError::Character(' '))
        );
        assert_eq!(
            NpcName::try_from("Éclair".to_owned()),
            Err(NpcNameError::Character('É'))
        );
    }

    #[test]
    fn a_key_holds_one_to_sixty_four_characters() {
        assert!(NpcKey::try_from("agent:7/task".to_owned()).is_ok());
        assert_eq!(NpcKey::try_from(String::new()), Err(NpcKeyError::Length(0)));
        assert_eq!(
            NpcKey::try_from("k".repeat(65)),
            Err(NpcKeyError::Length(65))
        );
    }
}
