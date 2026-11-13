use std::collections::HashMap;

use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::poi::{Occupancy, PoiRecord};
use rspigot_domain::{BlockPos, EntityId, SectionPos, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

use crate::BlockView;
use crate::brain::{AiContext, Brain};
use crate::random::{MobRandom, RandomSource};
use crate::world::{AiWorld, Facts, Living, MobBody, MobView, TargetKind};

#[derive(Default)]
pub(crate) struct Mock {
    pub players: Vec<Living>,
    pub others: Vec<Living>,
    pub levels: HashMap<SectionPos, i32>,
    pub pois: Vec<PoiRecord>,
    pub wanting: Vec<Living>,
    pub reputation: HashMap<(EntityId, EntityId), i32>,
    pub bright: bool,
    pub dark: bool,
    pub universal_anger: bool,
    pub blocks: HashMap<BlockPos, StateId>,
    pub peaceful: bool,
}

impl BlockView for Mock {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if (60..64).contains(&pos.y) {
            BlockKind::GrassBlock.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for Mock {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.id == id)
            .copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        let pool = if kind == TargetKind::Player {
            &self.players
        } else {
            &self.others
        };
        pool.iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        if self.peaceful { 0 } else { 2 }
    }

    fn game_time(&self) -> i64 {
        100
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn bright_outside(&self) -> bool {
        self.bright
    }

    fn dark_outside(&self) -> bool {
        self.dark
    }

    fn universal_anger(&self) -> bool {
        self.universal_anger
    }

    fn sections_to_village(&self, pos: BlockPos) -> i32 {
        self.levels
            .get(&SectionPos::of(pos))
            .copied()
            .unwrap_or(crate::world::NO_VILLAGE)
    }

    fn pois_in_range(&self, center: BlockPos, radius: i32, occupancy: Occupancy) -> Vec<PoiRecord> {
        let limit = i64::from(radius) * i64::from(radius);
        self.pois
            .iter()
            .filter(|record| occupancy.test(record))
            .filter(|record| {
                let (dx, dy, dz) = (
                    i64::from(record.pos().x - center.x),
                    i64::from(record.pos().y - center.y),
                    i64::from(record.pos().z - center.z),
                );
                dx * dx + dy * dy + dz * dz <= limit
            })
            .copied()
            .collect()
    }

    fn villagers_wanting_golem(&self, area: &Aabb) -> Vec<Living> {
        self.wanting
            .iter()
            .filter(|villager| villager.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn villager_reputation(&self, villager: EntityId, player: EntityId) -> i32 {
        self.reputation
            .get(&(villager, player))
            .copied()
            .unwrap_or(0)
    }
}

pub(crate) fn living(id: i32, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id: EntityId(id),
        player: kind == EntityKind::Player,
        kind,
        position,
        eye_height: f64::from(size.eye_height),
        bounds: Aabb::of_size(position, size.width, size.height),
        alive: true,
        spectator: false,
        creative: false,
        invulnerable: false,
        discrete: false,
        invisible: false,
        armor_cover: 0.0,
        main_hand: None,
        off_hand: None,
        yaw: 0.0,
        pitch: 0.0,
        in_water: false,
        baby: false,
        age: 0,
        in_love: false,
        panicking: false,
        has_target: false,
        scared: false,
        facts: Facts {
            uuid: Uuid::from_u128(id as u128 + 1000),
            ..Facts::default()
        },
    }
}

pub(crate) fn mob_view(kind: EntityKind, position: Vec3) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    MobView::of(&base, &state)
}

pub(crate) fn seed_where(check: impl Fn(&mut MobRandom) -> bool) -> i64 {
    (0..2_000_000)
        .find(|seed| check(&mut MobRandom::new(*seed)))
        .expect("a matching seed exists")
}

pub(crate) struct Harness {
    pub mob: MobView,
    pub world: Mock,
    pub brain: Brain,
    pub body: MobBody,
}

impl Harness {
    pub(crate) fn new(kind: EntityKind, position: Vec3, world: Mock, seed: i64) -> Self {
        let mob = mob_view(kind, position);
        let body = MobBody::of(&mob);
        Self {
            mob,
            world,
            brain: Brain::new(kind, seed),
            body,
        }
    }

    pub(crate) fn run<R>(&mut self, run: impl FnOnce(&mut AiContext<'_, Mock>) -> R) -> R {
        let mut context = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.brain,
            body: &mut self.body,
        };
        run(&mut context)
    }
}

pub(crate) fn first_draw_zero(bound: i32) -> impl Fn(&mut MobRandom) -> bool {
    move |random| random.next_int(bound) == 0
}
