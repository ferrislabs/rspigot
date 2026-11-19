pub const HEART: &str = "❤";
pub const HEART_COLOR: &str = "§c";
pub const LIVES_LABEL: &str = "§7Vies : ";

pub fn lives_line(lives: u32) -> String {
    let hearts = format!("{HEART_COLOR}{HEART} ").repeat(lives as usize);
    format!("{LIVES_LABEL}{}", hearts.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_heart_per_life() {
        assert_eq!(lives_line(3).matches(HEART).count(), 3);
        assert_eq!(lives_line(0), LIVES_LABEL);
    }
}
