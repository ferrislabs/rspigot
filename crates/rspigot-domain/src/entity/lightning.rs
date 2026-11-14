pub const LIFE_TICKS: i32 = 2;
pub const DAMAGE: f32 = 5.0;
pub const IGNITE_SECONDS: f32 = 8.0;
pub const FIRE_SPREAD: i32 = 4;
pub const HIT_RADIUS: f64 = 3.0;
pub const HIT_HEIGHT: f64 = 6.0;
pub const FLASH_GAP: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightningState {
    pub life: i32,
    pub flashes: i32,
    pub visual_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LightningStep {
    pub strike: bool,
    pub fire: Option<i32>,
    pub hit: bool,
    pub discard: bool,
}

impl LightningState {
    pub fn new(flashes: i32) -> Self {
        Self {
            life: LIFE_TICKS,
            flashes,
            visual_only: false,
        }
    }

    pub fn tick(&mut self, gap: i32) -> LightningStep {
        let mut step = LightningStep {
            strike: self.life == LIFE_TICKS,
            ..LightningStep::default()
        };
        self.life -= 1;
        if self.life < 0 {
            if self.flashes == 0 {
                step.discard = true;
            } else if self.life < -gap {
                self.flashes -= 1;
                self.life = 1;
                step.fire = Some(0);
            }
        }
        step.hit = self.life >= 0 && !self.visual_only && !step.discard;
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_tick_strikes_and_hits_then_the_bolt_flashes_and_goes_out() {
        let mut state = LightningState::new(1);
        let first = state.tick(0);
        assert!(first.strike && first.hit && !first.discard);
        let second = state.tick(0);
        assert!(!second.strike && second.hit);
        let flash = state.tick(0);
        assert_eq!(flash.fire, Some(0));
        assert!(!flash.discard);
        assert_eq!(state.flashes, 0);
        state.tick(0);
        state.tick(0);
        let last = state.tick(0);
        assert!(last.discard && !last.hit);
    }

    #[test]
    fn a_visual_bolt_never_hits() {
        let mut state = LightningState::new(1);
        state.visual_only = true;
        assert!(!state.tick(0).hit);
    }
}
