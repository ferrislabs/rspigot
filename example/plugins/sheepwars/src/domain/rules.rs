#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RespawnRule {
    Lives(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundRules {
    pub min_players: usize,
    pub max_players: usize,
    pub countdown_ticks: u32,
    pub sheep_interval_ticks: u32,
    pub respawn: RespawnRule,
}

impl Default for RoundRules {
    fn default() -> Self {
        Self {
            min_players: 2,
            max_players: 16,
            countdown_ticks: 20 * 15,
            sheep_interval_ticks: 20 * 20,
            respawn: RespawnRule::Lives(3),
        }
    }
}
