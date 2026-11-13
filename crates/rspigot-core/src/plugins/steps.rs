use crossbeam_channel::Sender;
use futures_channel::oneshot;
use rspigot_domain::effect::EffectInstance;
use rspigot_domain::entity::reward::EquipmentDrop;
use rspigot_domain::entity::{Entity, EntityBase};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{ClientSettings, GameMode, PlayerId};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Rotation, Vec3};

use crate::combat::{Strike, Victim};
use crate::commands::{CommandSender, Target};
use crate::events::{Direction, Hand, InputState, PreLoginVerdict};
use crate::explosion::BlockInteraction;
use crate::health::DamageSource;
use crate::plugins::EventKind;
use crate::plugins::broadcast::Recipient;
use crate::plugins::clicks::{CreativePacket, CreativeStep, DragStep, WindowClick, WindowPacket};
use crate::plugins::events::{CreatureSpawnReason, PlayerTeleportCause};
use crate::plugins::furnaces::{ExtractStep, FurnaceStep};
use crate::plugins::prepare::PrepareStep;
use crate::plugins::windows::{ButtonPacket, ClosePacket, OpenStep, PluginOpenStep};
use crate::status::StatusReply;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Held {
    pub(crate) position: Option<Vec3>,
    pub(crate) rotation: Option<Rotation>,
    pub(crate) on_ground: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MoveStep {
    pub(crate) player: PlayerId,
    pub(crate) from: Vec3,
    pub(crate) to: Vec3,
    pub(crate) rotation: Option<Rotation>,
    pub(crate) on_ground: bool,
}

#[derive(Debug)]
pub(crate) struct LeaveStep {
    pub(crate) name: String,
    pub(crate) default: String,
    pub(crate) vanilla: bool,
}

#[derive(Debug)]
pub(crate) struct KickStep {
    pub(crate) player: PlayerId,
    pub(crate) reason: Component,
    pub(crate) text: String,
    pub(crate) leave: String,
}

#[derive(Debug)]
pub(crate) struct SettingsStep {
    pub(crate) kind: EventKind,
    pub(crate) player: PlayerId,
    pub(crate) settings: Option<ClientSettings>,
}

#[derive(Debug)]
pub(crate) struct ChatStep {
    pub(crate) player: PlayerId,
    pub(crate) message: String,
}

#[derive(Debug)]
pub(crate) struct TeleportStep {
    pub(crate) player: PlayerId,
    pub(crate) position: Vec3,
    pub(crate) rotation: Option<Rotation>,
    pub(crate) cause: PlayerTeleportCause,
    pub(crate) fallback: Option<MoveStep>,
}

#[derive(Debug)]
pub(crate) struct VelocityStep {
    pub(crate) player: PlayerId,
    pub(crate) restore: Option<Vec3>,
}

#[derive(Debug)]
pub(crate) struct RespawnStep {
    pub(crate) player: PlayerId,
}

#[derive(Debug)]
pub(crate) struct DeathStep {
    pub(crate) player: PlayerId,
    pub(crate) message: Component,
    pub(crate) text: String,
    pub(crate) show: bool,
    pub(crate) keep: bool,
    pub(crate) inventory: Vec<(usize, ItemStack)>,
    pub(crate) experience: i32,
}

#[derive(Debug)]
pub(crate) struct InputStep {
    pub(crate) player: PlayerId,
    pub(crate) state: InputState,
}

#[derive(Debug)]
pub(crate) struct ToggleStep {
    pub(crate) player: PlayerId,
    pub(crate) value: bool,
}

#[derive(Debug)]
pub(crate) struct GameModeStep {
    pub(crate) player: PlayerId,
    pub(crate) mode: GameMode,
    pub(crate) replies: Vec<(CommandSender, Component)>,
}

#[derive(Debug)]
pub(crate) struct HeldStep {
    pub(crate) player: PlayerId,
    pub(crate) slot: u8,
    pub(crate) previous: Option<u8>,
}

#[derive(Debug)]
pub(crate) struct SwingStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
}

#[derive(Debug)]
pub(crate) struct SwapStep {
    pub(crate) player: PlayerId,
    pub(crate) selected: u8,
    pub(crate) main: Option<ItemStack>,
    pub(crate) off: Option<ItemStack>,
}

#[derive(Debug)]
pub(crate) struct UseOnStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) slot: usize,
    pub(crate) pos: BlockPos,
    pub(crate) face: Direction,
    pub(crate) cursor: Vec3,
    pub(crate) sequence: i32,
    pub(crate) stack: Option<ItemStack>,
    pub(crate) clicked: BlockStateId,
    pub(crate) game_mode: GameMode,
    pub(crate) block_denied: bool,
}

#[derive(Debug)]
pub(crate) struct ClickStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) pos: BlockPos,
    pub(crate) face: Direction,
    pub(crate) cursor: Vec3,
    pub(crate) sequence: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Aim {
    pub(crate) pos: BlockPos,
    pub(crate) face: Direction,
    pub(crate) clicked: Vec3,
}

#[derive(Debug)]
pub(crate) struct UseStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) sequence: i32,
    pub(crate) rotation: Rotation,
    pub(crate) aim: Option<Aim>,
}

#[derive(Debug)]
pub(crate) struct DigStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
    pub(crate) clicked: BlockStateId,
    pub(crate) game_mode: GameMode,
}

#[derive(Debug)]
pub(crate) struct InteractStep {
    pub(crate) player: PlayerId,
    pub(crate) target: EntityId,
    pub(crate) hand: Hand,
    pub(crate) stack: Option<ItemStack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropSource {
    Selected { slot: usize },
    Creative,
    Released,
}

#[derive(Debug)]
pub(crate) struct Launch {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) yaw: f32,
}

#[derive(Debug)]
pub(crate) struct Announced {
    pub(crate) base: EntityBase,
    pub(crate) velocity: Vec3,
    pub(crate) yaw: f32,
}

#[derive(Debug)]
pub(crate) struct SelectedDrop {
    pub(crate) player: PlayerId,
    pub(crate) all: bool,
}

#[derive(Debug)]
pub(crate) struct DropStep {
    pub(crate) player: PlayerId,
    pub(crate) stack: ItemStack,
    pub(crate) source: DropSource,
    pub(crate) launch: Launch,
    pub(crate) thrower: bool,
    pub(crate) predicted: bool,
}

#[derive(Debug)]
pub(crate) struct WearRequest {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) slot: usize,
    pub(crate) amount: i32,
}

#[derive(Debug)]
pub(crate) struct WearStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) slot: usize,
    pub(crate) stack: ItemStack,
    pub(crate) damage: i32,
}

#[derive(Debug)]
pub(crate) struct BookStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) window: i32,
    pub(crate) book: ItemStack,
}

#[derive(Debug)]
pub(crate) struct CooldownStep {
    pub(crate) player: PlayerId,
    pub(crate) value: i32,
}

#[derive(Debug)]
pub(crate) struct OrbStep {
    pub(crate) player: PlayerId,
    pub(crate) collector: EntityId,
    pub(crate) orb: EntityId,
    pub(crate) amount: i32,
}

#[derive(Debug)]
pub(crate) struct ArrowStep {
    pub(crate) player: PlayerId,
    pub(crate) collector: EntityId,
    pub(crate) arrow: EntityId,
}

#[derive(Debug)]
pub(crate) struct ShearStep {
    pub(crate) player: PlayerId,
    pub(crate) target: EntityId,
    pub(crate) hand: Hand,
    pub(crate) stack: ItemStack,
    pub(crate) parked: bool,
}

#[derive(Debug)]
pub(crate) struct BucketEntityStep {
    pub(crate) player: PlayerId,
    pub(crate) target: EntityId,
    pub(crate) hand: Hand,
    pub(crate) stack: ItemStack,
    pub(crate) bucket: ItemStack,
    pub(crate) parked: bool,
}

#[derive(Debug)]
pub(crate) struct BucketEmptyStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) stack: ItemStack,
    pub(crate) kind: rspigot_entities::EntityKind,
    pub(crate) block: BlockPos,
    pub(crate) placement: crate::bucket::Placement,
}

#[derive(Debug)]
pub(crate) struct DamageStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
    pub(crate) state: BlockStateId,
    pub(crate) held: Option<ItemStack>,
    pub(crate) insta: bool,
}

#[derive(Debug)]
pub(crate) struct BreakStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
    pub(crate) state: BlockStateId,
    pub(crate) game_mode: GameMode,
    pub(crate) held: Option<ItemStack>,
    pub(crate) refused: bool,
    pub(crate) exp: i32,
}

#[derive(Debug)]
pub(crate) struct FinishStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
}

#[derive(Debug)]
pub(crate) struct AbortStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
}

#[derive(Debug)]
pub(crate) struct DropItemsStep {
    pub(crate) pos: BlockPos,
    pub(crate) drops: Vec<(Announced, ItemStack)>,
    pub(crate) exp: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) pos: BlockPos,
    pub(crate) old: BlockStateId,
    pub(crate) new: BlockStateId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolEffects {
    pub(crate) sound: &'static str,
    pub(crate) particle: Option<i32>,
    pub(crate) damage: i32,
    pub(crate) face: Direction,
}

#[derive(Debug)]
pub(crate) struct UseItem {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) slot: usize,
    pub(crate) stack: ItemStack,
    pub(crate) against: BlockPos,
    pub(crate) sequence: i32,
    pub(crate) game_mode: GameMode,
}

#[derive(Debug)]
pub(crate) struct BuildStep {
    pub(crate) using: UseItem,
    pub(crate) target: BlockPos,
    pub(crate) old: BlockStateId,
    pub(crate) state: BlockStateId,
    pub(crate) linked: Option<(BlockPos, BlockStateId)>,
    pub(crate) buildable: bool,
}

#[derive(Debug)]
pub(crate) struct PlaceStep {
    pub(crate) using: UseItem,
    pub(crate) changes: Vec<Change>,
    pub(crate) build: bool,
    pub(crate) tool: Option<ToolEffects>,
    pub(crate) loot: Vec<ItemStack>,
    pub(crate) forced: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PhysicsStep {
    pub(crate) pos: BlockPos,
    pub(crate) state: BlockStateId,
}

#[derive(Debug)]
pub(crate) struct CookStep {
    pub(crate) pos: BlockPos,
    pub(crate) slot: usize,
    pub(crate) source: ItemStack,
    pub(crate) result: ItemStack,
}

#[derive(Debug)]
pub(crate) struct CampfireStep {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) slot: usize,
    pub(crate) pos: BlockPos,
    pub(crate) sequence: i32,
    pub(crate) held: ItemStack,
    pub(crate) game_mode: GameMode,
    pub(crate) cooking_slot: usize,
    pub(crate) total: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExpStep {
    pub(crate) pos: BlockPos,
    pub(crate) amount: i32,
    pub(crate) at: Vec3,
}

#[derive(Debug)]
pub(crate) struct ExplodeStep {
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
    pub(crate) interaction: BlockInteraction,
    pub(crate) source: Option<EntityId>,
    pub(crate) blocks: Vec<BlockPos>,
    pub(crate) knocked: Vec<(PlayerId, Vec3)>,
    pub(crate) explosion_yield: f32,
    pub(crate) fire: bool,
    pub(crate) discard: Option<EntityId>,
}

#[derive(Debug)]
pub(crate) struct BreedStep {
    pub(crate) parent: EntityId,
    pub(crate) partner: EntityId,
    pub(crate) child: Entity,
    pub(crate) experience: i32,
    pub(crate) position: Vec3,
}

#[derive(Debug)]
pub(crate) struct LoveStep {
    pub(crate) target: EntityId,
    pub(crate) cause: Option<uuid::Uuid>,
    pub(crate) item: Option<ItemStack>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PigAngerStep {
    pub(crate) mob: EntityId,
    pub(crate) anger: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RideStep {
    pub(crate) rider: EntityId,
    pub(crate) vehicle: EntityId,
    pub(crate) force: bool,
    pub(crate) notify: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpellStep {
    pub(crate) caster: EntityId,
    pub(crate) spell: rspigot_domain::entity::species::spellcaster::IllagerSpell,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TameStep {
    pub(crate) target: EntityId,
    pub(crate) owner: PlayerId,
}

#[derive(Debug)]
pub(crate) struct DyeStep {
    pub(crate) player: PlayerId,
    pub(crate) target: EntityId,
    pub(crate) hand: Hand,
    pub(crate) color: u8,
    pub(crate) stack: ItemStack,
    pub(crate) parked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seeker {
    Mob,
    Tempt,
    Orb,
    WitherHead(u8),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AimStep {
    pub(crate) mob: EntityId,
    pub(crate) from: Option<EntityId>,
    pub(crate) to: Option<EntityId>,
    pub(crate) seeker: Seeker,
    pub(crate) deferred: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shooter {
    Player(PlayerId),
    Mob(EntityId),
    Crossbow {
        mob: EntityId,
        hand: Hand,
        wear: i32,
    },
}

#[derive(Debug)]
pub(crate) struct Draw {
    pub(crate) weapon: Option<ItemStack>,
    pub(crate) ammo: Option<ItemStack>,
    pub(crate) hand: Hand,
    pub(crate) force: f32,
}

#[derive(Debug)]
pub(crate) struct ShotStep {
    pub(crate) shooter: Shooter,
    pub(crate) force: f32,
    pub(crate) arrow: Entity,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PrimeStep {
    pub(crate) source: EntityId,
    pub(crate) radius: f32,
    pub(crate) fire: bool,
    pub(crate) kind: PrimeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimeKind {
    Creeper,
    Projectile,
    WitherSpawn,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShulkerHop {
    pub(crate) face: rspigot_blocks::Face,
    pub(crate) breed: Option<(Vec3, rspigot_domain::entity::Aabb)>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RelocateStep {
    pub(crate) entity: EntityId,
    pub(crate) to: Vec3,
    pub(crate) command: bool,
    pub(crate) from: Option<Vec3>,
    pub(crate) shulker: Option<ShulkerHop>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TimeStep {
    pub(crate) skipped: i64,
    pub(crate) applied: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Sky {
    pub(crate) raining: bool,
    pub(crate) thundering: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WeatherStep {
    pub(crate) rain: Option<bool>,
    pub(crate) thunder: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Follow {
    Nothing,
    Infection { at: Option<BlockPos> },
    Cure { starter: Option<uuid::Uuid> },
    Metamorphosis,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitStep {
    pub(crate) id: EntityId,
    pub(crate) count: i32,
}

#[derive(Debug)]
pub(crate) struct ScatterStep {
    pub(crate) id: EntityId,
    pub(crate) children: Vec<Entity>,
}

#[derive(Debug)]
pub(crate) struct TransformStep {
    pub(crate) old: EntityId,
    pub(crate) converted: Entity,
    pub(crate) birth: Birth,
}

#[derive(Debug)]
pub(crate) enum Spawned {
    Nothing,
    Summoned {
        sender: CommandSender,
        name: Component,
    },
    Hatched {
        player: PlayerId,
        slot: usize,
        item: ItemStack,
    },
    Converted {
        old: EntityId,
        event: Option<i32>,
        follow: Follow,
    },
    Answered {
        connection: u64,
        request: u64,
    },
    Poof,
    Mounted {
        rider: EntityId,
    },
    Rides {
        vehicle: EntityId,
    },
    Pattern(Vec<(BlockPos, BlockStateId)>),
    Hive(Box<HiveRelease>),
}

#[derive(Debug)]
pub(crate) struct HiveRelease {
    pub(crate) pos: BlockPos,
    pub(crate) occupant: rspigot_domain::beehive::Occupant,
    pub(crate) status: rspigot_domain::beehive::ReleaseStatus,
    pub(crate) angered_by: Option<PlayerId>,
}

#[derive(Debug)]
pub(crate) struct Birth {
    pub(crate) reason: CreatureSpawnReason,
    pub(crate) random: Option<LegacyRandom>,
    pub(crate) cause: Option<PlayerId>,
    pub(crate) then: Spawned,
}

impl Birth {
    pub(crate) const fn of(reason: CreatureSpawnReason) -> Self {
        Self {
            reason,
            random: None,
            cause: None,
            then: Spawned::Nothing,
        }
    }

    pub(crate) fn with_random(self, random: LegacyRandom) -> Self {
        Self {
            random: Some(random),
            ..self
        }
    }

    pub(crate) fn by(self, cause: PlayerId) -> Self {
        Self {
            cause: Some(cause),
            ..self
        }
    }

    pub(crate) fn then(self, then: Spawned) -> Self {
        Self { then, ..self }
    }
}

#[derive(Debug)]
pub(crate) struct SpawnStep {
    pub(crate) kind: EventKind,
    pub(crate) entity: Entity,
    pub(crate) loaded: bool,
    pub(crate) birth: Birth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Struck {
    Player(PlayerId),
    Mob(EntityId),
    Item(EntityId),
    Projectile(EntityId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Hurt,
    Missed,
    Cancelled,
}

impl Outcome {
    pub(crate) const fn hurt(self) -> bool {
        matches!(self, Self::Hurt)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AttackFollow {
    pub(crate) attacker: PlayerId,
    pub(crate) attacker_entity: EntityId,
    pub(crate) victim: Victim,
    pub(crate) strike: Strike,
    pub(crate) full: bool,
    pub(crate) knockback: f32,
    pub(crate) yaw: f32,
    pub(crate) sweep: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SweepFollow {
    pub(crate) attacker: PlayerId,
    pub(crate) target: Target,
    pub(crate) direction: (f64, f64),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MobAttackFollow {
    pub(crate) attacker: EntityId,
    pub(crate) target: Target,
    pub(crate) knockback: f32,
    pub(crate) yaw: f32,
    pub(crate) golem: bool,
    pub(crate) hoglin: bool,
    pub(crate) bee: bool,
    pub(crate) puffer: Option<i32>,
    pub(crate) cube: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RamFollow {
    pub(crate) goat: EntityId,
    pub(crate) target: Target,
    pub(crate) strength: f64,
    pub(crate) direction: (f64, f64),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArrowFollow {
    pub(crate) arrow: EntityId,
    pub(crate) target: EntityId,
    pub(crate) owner: Option<EntityId>,
    pub(crate) saved_fire: i32,
    pub(crate) sound_pitch: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThrownFollow {
    pub(crate) projectile: EntityId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BulletFollow {
    pub(crate) projectile: EntityId,
    pub(crate) target: EntityId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HurtingFollow {
    pub(crate) projectile: EntityId,
    pub(crate) target: EntityId,
    pub(crate) owner: Option<EntityId>,
    pub(crate) saved_fire: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IgniteStep {
    pub(crate) projectile: EntityId,
    pub(crate) pos: BlockPos,
    pub(crate) state: BlockStateId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BurnFollow {
    pub(crate) position: Vec3,
    pub(crate) player: bool,
    pub(crate) entity: EntityId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlastFollow {
    pub(crate) target: EntityId,
    pub(crate) player: Option<PlayerId>,
    pub(crate) knockback: Vec3,
    pub(crate) source: Option<EntityId>,
}

#[derive(Debug)]
pub(crate) enum AfterHurt {
    Nothing,
    Attack(Box<AttackFollow>),
    Sweep(SweepFollow),
    MobAttack(MobAttackFollow),
    Ram(RamFollow),
    Arrow(ArrowFollow),
    Thrown(ThrownFollow),
    Hurting(HurtingFollow),
    Bullet(BulletFollow),
    Burn(BurnFollow),
    Blast(BlastFollow),
}

#[derive(Debug)]
pub(crate) struct HurtStep {
    pub(crate) kind: EventKind,
    pub(crate) victim: Struck,
    pub(crate) source: DamageSource,
    pub(crate) amount: f32,
    pub(crate) then: AfterHurt,
}

impl HurtStep {
    fn repeats(&self) -> bool {
        matches!(
            self.source.kind,
            rspigot_domain::damage::DamageType::Lava
                | rspigot_domain::damage::DamageType::OutOfWorld
                | rspigot_domain::damage::DamageType::InFire
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegainStep {
    pub(crate) player: PlayerId,
    pub(crate) amount: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MobRegainStep {
    pub(crate) target: EntityId,
    pub(crate) amount: f32,
    pub(crate) repair: Option<GolemRepair>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GolemRepair {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExhaustStep {
    pub(crate) player: PlayerId,
    pub(crate) amount: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FoodStep {
    pub(crate) player: PlayerId,
    pub(crate) level: i32,
    pub(crate) from: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holder {
    Player(PlayerId),
    Mob(EntityId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Potion {
    Add(EffectInstance),
    Remove(EffectInstance),
    Clear(EffectInstance),
    Expire(EffectInstance),
}

#[derive(Debug)]
pub(crate) struct Landing {
    pub(crate) falling: EntityId,
    pub(crate) pos: BlockPos,
    pub(crate) original: BlockStateId,
    pub(crate) replaced: BlockStateId,
    pub(crate) state: BlockStateId,
    pub(crate) drop_item: bool,
}

#[derive(Debug)]
pub(crate) enum Grief {
    Fall {
        pos: BlockPos,
        state: BlockStateId,
        entity: Box<Entity>,
    },
    Land(Box<Landing>),
    Graze {
        sheep: EntityId,
        pos: BlockPos,
        below: bool,
    },
    Raid {
        rabbit: EntityId,
        pos: BlockPos,
    },
    Berries {
        fox: EntityId,
        pos: BlockPos,
        glow: bool,
    },
    Merge {
        silverfish: EntityId,
        pos: BlockPos,
    },
    WakeFriends {
        silverfish: EntityId,
        origin: BlockPos,
        next: u32,
        pos: BlockPos,
    },
    EndermanTake {
        enderman: EntityId,
        pos: BlockPos,
        carried: BlockStateId,
    },
    EndermanPlace {
        enderman: EntityId,
        pos: BlockPos,
        state: BlockStateId,
    },
    Douse {
        potion: EntityId,
        pos: BlockPos,
        to: BlockStateId,
    },
    Compost(Box<crate::compost::CompostRun>),
    Statue {
        golem: EntityId,
        pos: BlockPos,
        state: BlockStateId,
    },
    RavagerBreak {
        ravager: EntityId,
        pos: BlockPos,
    },
    WitherBreak {
        wither: EntityId,
        pos: BlockPos,
    },
    BeeGrow {
        bee: EntityId,
        pos: BlockPos,
        to: BlockStateId,
    },
    TurtleEgg {
        turtle: EntityId,
        at: BlockPos,
        pos: BlockPos,
        state: BlockStateId,
    },
    Frogspawn {
        frog: EntityId,
        pos: BlockPos,
        state: BlockStateId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplashHit {
    Block {
        pos: BlockPos,
        face: rspigot_blocks::Face,
    },
    Entity(EntityId),
}

#[derive(Debug)]
pub(crate) struct SplashStep {
    pub(crate) potion: EntityId,
    pub(crate) owner: Option<EntityId>,
    pub(crate) hit: SplashHit,
    pub(crate) effects: Vec<EffectInstance>,
    pub(crate) scale: f32,
    pub(crate) affected: Vec<(EntityId, f64)>,
    pub(crate) color: i32,
    pub(crate) instant: bool,
    pub(crate) silent: bool,
    pub(crate) pos: BlockPos,
}

#[derive(Debug)]
pub(crate) struct GatherStep {
    pub(crate) fox: EntityId,
    pub(crate) item: EntityId,
    pub(crate) stack: ItemStack,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GrowStep {
    pub(crate) pos: BlockPos,
    pub(crate) state: BlockStateId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FormStep {
    pub(crate) entity: EntityId,
    pub(crate) pos: BlockPos,
    pub(crate) state: BlockStateId,
}

#[derive(Debug)]
pub(crate) struct ChangeStep {
    pub(crate) change: Grief,
}

#[derive(Debug)]
pub(crate) struct PickupStep {
    pub(crate) player: PlayerId,
    pub(crate) collector: EntityId,
    pub(crate) item: EntityId,
    pub(crate) stack: ItemStack,
}

#[derive(Debug)]
pub(crate) struct MobPickupStep {
    pub(crate) collector: EntityId,
    pub(crate) item: EntityId,
    pub(crate) stack: ItemStack,
}

#[derive(Debug)]
pub(crate) struct PiglinPickupStep {
    pub(crate) collector: EntityId,
    pub(crate) item: EntityId,
    pub(crate) stack: ItemStack,
    pub(crate) whole: bool,
}

#[derive(Debug)]
pub(crate) struct AcquireStep {
    pub(crate) merchant: EntityId,
    pub(crate) offer: rspigot_domain::entity::trading::MerchantOffer,
}

#[derive(Debug)]
pub(crate) struct ReplenishStep {
    pub(crate) merchant: EntityId,
    pub(crate) index: usize,
    pub(crate) uses: i32,
}

#[derive(Debug)]
pub(crate) struct SelectTradeStep {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
    pub(crate) index: i32,
}

#[derive(Debug)]
pub(crate) struct BarterStep {
    pub(crate) piglin: EntityId,
    pub(crate) outcome: Vec<ItemStack>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Push {
    Knock {
        strength: f64,
        direction: (f64, f64),
    },
    Blast(Vec3),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KnockStep {
    pub(crate) kind: EventKind,
    pub(crate) target: Holder,
    pub(crate) push: Push,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Igniter {
    Nothing,
    Block(Option<BlockPos>),
    Entity(EntityId),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CombustStep {
    pub(crate) kind: EventKind,
    pub(crate) target: Holder,
    pub(crate) igniter: Igniter,
    pub(crate) seconds: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PotionStep {
    pub(crate) holder: Holder,
    pub(crate) potion: Potion,
    pub(crate) overrides: bool,
    pub(crate) respawned: bool,
}

#[derive(Debug)]
pub(crate) struct LootStep {
    pub(crate) victim: EntityId,
    pub(crate) drops: Vec<ItemStack>,
    pub(crate) slots: Vec<EquipmentDrop>,
    pub(crate) experience: i32,
}

#[derive(Debug)]
pub(crate) struct PingStep {
    pub(crate) motd: String,
    pub(crate) legacy: bool,
    pub(crate) reply: Option<oneshot::Sender<StatusReply>>,
}

#[derive(Debug)]
pub(crate) struct SuggestStep {
    pub(crate) player: PlayerId,
    pub(crate) request: i32,
    pub(crate) start: i32,
    pub(crate) length: i32,
    pub(crate) matches: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum Continuation {
    PlayerMove(MoveStep),
    Notice(EventKind),
    PreLogin(Sender<PreLoginVerdict>),
    Login(u64),
    Join(PlayerId),
    Leave(LeaveStep),
    Kick(KickStep),
    Settings(SettingsStep),
    CommandSend(PlayerId, Vec<String>),
    Chat(ChatStep, Vec<PlayerId>),
    Command(ChatStep),
    Teleport(TeleportStep),
    Velocity(VelocityStep),
    Respawn(RespawnStep),
    Death(Box<DeathStep>),
    Input(InputStep),
    Sneak(ToggleStep),
    Sprint(ToggleStep),
    Flight(ToggleStep),
    GameMode(GameModeStep),
    HeldSlot(HeldStep),
    Swing(SwingStep),
    Swap(Box<SwapStep>),
    ClickBlock(Box<ClickStep>),
    UseItemOn(Box<UseOnStep>),
    UseItem(UseStep),
    Dig(DigStep),
    InteractEntity(Box<InteractStep>),
    DropSelected(SelectedDrop),
    Drop(Box<DropStep>),
    Wear(WearRequest),
    ItemDamage(Box<WearStep>),
    TakeBook(Box<BookStep>),
    ExpCooldown(CooldownStep),
    ExpChange(OrbStep),
    PickupArrow(ArrowStep),
    Shear(Box<ShearStep>),
    BucketEntity(Box<BucketEntityStep>),
    BucketEmpty(Box<BucketEmptyStep>),
    Damage(Box<DamageStep>),
    Break(Box<BreakStep>),
    Finish(FinishStep),
    Abort(AbortStep),
    DropItems(Box<DropItemsStep>),
    CanBuild(Box<BuildStep>),
    Place(Box<PlaceStep>),
    MultiPlace(Box<PlaceStep>),
    Physics(PhysicsStep),
    Cook(Box<CookStep>),
    CampfireStart(Box<CampfireStep>),
    Exp(ExpStep),
    Explode(Box<ExplodeStep>),
    ChunkUnload(ChunkPos),
    TimeSkip(TimeStep),
    SetWeather(Sky),
    Weather(WeatherStep),
    Thunder(bool),
    Spawn(Box<SpawnStep>),
    EntityDrop(Box<Entity>),
    Hurt(Box<HurtStep>),
    Strike(Box<HurtStep>),
    MobDeath(Box<LootStep>),
    Regain(RegainStep),
    MobRegain(MobRegainStep),
    Exhaust(ExhaustStep),
    Food(FoodStep),
    Potion(PotionStep),
    Combust(CombustStep),
    Knock(KnockStep),
    ItemDespawn(EntityId),
    PickupItem(Box<PickupStep>),
    Gather(Box<GatherStep>),
    MobPickup(Box<MobPickupStep>),
    PiglinPickup(Box<PiglinPickupStep>),
    Barter(Box<BarterStep>),
    Acquire(Box<AcquireStep>),
    Replenish(ReplenishStep),
    SelectTrade(SelectTradeStep),
    Transform(Box<TransformStep>),
    Split(SplitStep),
    Scatter(Box<ScatterStep>),
    ChangeBlock(Box<ChangeStep>),
    Form(FormStep),
    Grow(GrowStep),
    Ignite(IgniteStep),
    Regrow(EntityId),
    SpellCast(SpellStep),
    Boarding(RideStep),
    Mounting(RideStep),
    Exiting(RideStep),
    Dismounting(RideStep),
    EggBreak {
        mob: EntityId,
        pos: BlockPos,
        trample: bool,
    },
    EnterHive {
        bee: EntityId,
        pos: BlockPos,
    },
    DoorOpen {
        mob: EntityId,
        pos: BlockPos,
    },
    RaidTrigger {
        player: PlayerId,
        pos: BlockPos,
        amplifier: u8,
    },
    TargetBlock {
        mob: EntityId,
        pos: BlockPos,
    },
    EntityExplode(Box<ExplodeStep>),
    Splash(Box<SplashStep>),
    Prime(PrimeStep),
    Relocate(RelocateStep),
    Shoot(Box<ShotStep>),
    ProjectileHit {
        arrow: EntityId,
        target: EntityId,
    },
    Aim(AimStep),
    Breed(Box<BreedStep>),
    Love(Box<LoveStep>),
    Tame(TameStep),
    PigAnger(PigAngerStep),
    Dye(Box<DyeStep>),
    Window(Box<WindowPacket>),
    Click(EventKind, Box<WindowClick>),
    Drag(Box<DragStep>),
    Furnace(EventKind, Box<FurnaceStep>),
    Extract(ExtractStep),
    Prepare(EventKind, Box<PrepareStep>),
    Open(Box<OpenStep>),
    OpenPlugin(PluginOpenStep),
    Close(ClosePacket),
    Button(ButtonPacket),
    CreativeSlot(Box<CreativePacket>),
    Creative(Box<CreativeStep>),
    Ping(Box<PingStep>),
    ServerCommand(String),
    TabComplete(Box<SuggestStep>),
    Broadcast(String, Vec<Recipient>),
}

impl Continuation {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::PlayerMove(_) => EventKind::PlayerMove,
            Self::Notice(kind) => *kind,
            Self::PreLogin(_) => EventKind::AsyncPlayerPreLogin,
            Self::Login(_) => EventKind::PlayerLogin,
            Self::Join(_) => EventKind::PlayerJoin,
            Self::Leave(_) => EventKind::PlayerLeave,
            Self::Kick(_) => EventKind::PlayerKick,
            Self::Settings(step) => step.kind,
            Self::CommandSend(..) => EventKind::PlayerCommandSend,
            Self::Chat(..) => EventKind::AsyncPlayerChat,
            Self::Command(_) => EventKind::PlayerCommandPreprocess,
            Self::Teleport(_) => EventKind::PlayerTeleport,
            Self::Velocity(_) => EventKind::PlayerVelocity,
            Self::Respawn(_) => EventKind::PlayerRespawn,
            Self::Death(_) => EventKind::PlayerDeath,
            Self::Input(_) => EventKind::PlayerInput,
            Self::Sneak(_) => EventKind::PlayerToggleSneak,
            Self::Sprint(_) => EventKind::PlayerToggleSprint,
            Self::Flight(_) => EventKind::PlayerToggleFlight,
            Self::GameMode(_) => EventKind::PlayerGameModeChange,
            Self::HeldSlot(_) => EventKind::PlayerItemHeld,
            Self::Swing(_) => EventKind::PlayerAnimation,
            Self::Swap(_) => EventKind::PlayerSwapHandItems,
            Self::ClickBlock(_) | Self::UseItemOn(_) | Self::UseItem(_) | Self::Dig(_) => {
                EventKind::PlayerInteract
            }
            Self::InteractEntity(_) => EventKind::PlayerInteractAtEntity,
            Self::DropSelected(_) | Self::Drop(_) => EventKind::PlayerDropItem,
            Self::Wear(_) | Self::ItemDamage(_) => EventKind::PlayerItemDamage,
            Self::TakeBook(_) => EventKind::PlayerTakeLecternBook,
            Self::ExpCooldown(_) => EventKind::PlayerExpCooldownChange,
            Self::ExpChange(_) => EventKind::PlayerExpChange,
            Self::PickupArrow(_) => EventKind::PlayerPickupArrow,
            Self::Shear(_) => EventKind::PlayerShearEntity,
            Self::BucketEntity(_) => EventKind::PlayerBucketEntity,
            Self::BucketEmpty(_) => EventKind::PlayerBucketEmpty,
            Self::Damage(_) => EventKind::BlockDamage,
            Self::Break(_) | Self::Finish(_) => EventKind::BlockBreak,
            Self::Abort(_) => EventKind::BlockDamageAbort,
            Self::DropItems(_) => EventKind::BlockDropItem,
            Self::CanBuild(_) => EventKind::BlockCanBuild,
            Self::Place(_) => EventKind::BlockPlace,
            Self::MultiPlace(_) => EventKind::BlockMultiPlace,
            Self::Physics(_) => EventKind::BlockPhysics,
            Self::Cook(_) => EventKind::BlockCook,
            Self::CampfireStart(_) => EventKind::CampfireStart,
            Self::Exp(_) => EventKind::BlockExp,
            Self::Explode(_) => EventKind::BlockExplode,
            Self::ChunkUnload(_) => EventKind::ChunkUnload,
            Self::TimeSkip(_) => EventKind::TimeSkip,
            Self::SetWeather(_) | Self::Weather(_) => EventKind::WeatherChange,
            Self::Thunder(_) => EventKind::ThunderChange,
            Self::Spawn(step) => step.kind,
            Self::EntityDrop(_) => EventKind::EntityDropItem,
            Self::Hurt(step) | Self::Strike(step) => step.kind,
            Self::MobDeath(_) => EventKind::EntityDeath,
            Self::Regain(_) | Self::MobRegain(_) => EventKind::EntityRegainHealth,
            Self::Exhaust(_) => EventKind::EntityExhaustion,
            Self::Food(_) => EventKind::FoodLevelChange,
            Self::Potion(_) => EventKind::EntityPotionEffect,
            Self::Combust(step) => step.kind,
            Self::Knock(step) => step.kind,
            Self::ItemDespawn(_) => EventKind::ItemDespawn,
            Self::PickupItem(_) | Self::Gather(_) | Self::MobPickup(_) | Self::PiglinPickup(_) => {
                EventKind::EntityPickupItem
            }
            Self::Barter(_) => EventKind::PiglinBarter,
            Self::Acquire(_) => EventKind::VillagerAcquireTrade,
            Self::Replenish(_) => EventKind::VillagerReplenishTrade,
            Self::SelectTrade(_) => EventKind::TradeSelect,
            Self::Transform(_) | Self::Scatter(_) => EventKind::EntityTransform,
            Self::Split(_) => EventKind::CubeMobSplit,
            Self::ChangeBlock(_) => EventKind::EntityChangeBlock,
            Self::Form(_) => EventKind::EntityBlockForm,
            Self::Grow(_) => EventKind::BlockGrow,
            Self::Ignite(_) => EventKind::BlockIgnite,
            Self::Regrow(_) => EventKind::SheepRegrowWool,
            Self::SpellCast(_) => EventKind::EntitySpellCast,
            Self::Boarding(_) => EventKind::VehicleEnter,
            Self::Mounting(_) => EventKind::EntityMount,
            Self::Exiting(_) => EventKind::VehicleExit,
            Self::Dismounting(_) => EventKind::EntityDismount,
            Self::EggBreak { .. } | Self::DoorOpen { .. } => EventKind::EntityInteract,
            Self::RaidTrigger { .. } => EventKind::RaidTrigger,
            Self::EnterHive { .. } => EventKind::EntityEnterBlock,
            Self::TargetBlock { .. } => EventKind::EntityTargetBlock,
            Self::EntityExplode(_) => EventKind::EntityExplode,
            Self::Splash(_) => EventKind::PotionSplash,
            Self::Prime(_) => EventKind::ExplosionPrime,
            Self::Relocate(_) => EventKind::EntityTeleport,
            Self::Shoot(_) => EventKind::EntityShootBow,
            Self::ProjectileHit { .. } => EventKind::ProjectileHit,
            Self::Aim(_) => EventKind::EntityTargetLivingEntity,
            Self::Breed(_) => EventKind::EntityBreed,
            Self::Love(_) => EventKind::EntityEnterLoveMode,
            Self::Tame(_) => EventKind::EntityTame,
            Self::PigAnger(_) => EventKind::PigZombieAnger,
            Self::Dye(_) => EventKind::SheepDyeWool,
            Self::Window(_) => EventKind::InventoryClick,
            Self::Click(kind, _) => *kind,
            Self::Drag(_) => EventKind::InventoryDrag,
            Self::Furnace(kind, _) => *kind,
            Self::Extract(_) => EventKind::FurnaceExtract,
            Self::Prepare(kind, _) => *kind,
            Self::Open(_) | Self::OpenPlugin(_) => EventKind::InventoryOpen,
            Self::Close(_) => EventKind::InventoryClose,
            Self::Button(_) => EventKind::InventoryClick,
            Self::CreativeSlot(_) | Self::Creative(_) => EventKind::InventoryCreative,
            Self::Ping(_) => EventKind::ServerListPing,
            Self::ServerCommand(_) => EventKind::ServerCommand,
            Self::TabComplete(_) => EventKind::TabComplete,
            Self::Broadcast(..) => EventKind::BroadcastMessage,
        }
    }

    pub(crate) const fn destination(&self) -> Option<Vec3> {
        match self {
            Self::PlayerMove(step) => Some(step.to),
            _ => None,
        }
    }

    pub(crate) const fn undo(&self) -> Option<Undo> {
        match self {
            Self::PlayerMove(step) => Some(Undo::SendBack(step.from)),
            _ => None,
        }
    }

    pub(crate) const fn is_move(&self) -> bool {
        matches!(self, Self::PlayerMove(_))
    }

    pub(crate) fn acts_on(&self, id: EntityId) -> bool {
        match self {
            Self::Spawn(step) => {
                matches!(step.birth.then, Spawned::Converted { old, .. } if old == id)
            }
            Self::Hurt(step) | Self::Strike(step) => {
                matches!(step.victim, Struck::Mob(victim) | Struck::Item(victim) | Struck::Projectile(victim) if victim == id)
            }
            Self::MobDeath(step) => step.victim == id,
            Self::Potion(PotionStep { holder, .. })
            | Self::Combust(CombustStep { target: holder, .. })
            | Self::Knock(KnockStep { target: holder, .. }) => *holder == Holder::Mob(id),
            Self::ItemDespawn(item) | Self::Regrow(item) => *item == id,
            Self::SpellCast(step) => step.caster == id,
            Self::Boarding(step)
            | Self::Mounting(step)
            | Self::Exiting(step)
            | Self::Dismounting(step) => step.rider == id || step.vehicle == id,
            Self::PickupItem(step) => step.item == id || step.collector == id,
            Self::Gather(step) => step.item == id || step.fox == id,
            Self::MobPickup(step) => step.item == id || step.collector == id,
            Self::PiglinPickup(step) => step.item == id || step.collector == id,
            Self::Barter(step) => step.piglin == id,
            Self::Acquire(step) => step.merchant == id,
            Self::Replenish(step) => step.merchant == id,
            Self::Transform(step) => step.old == id,
            Self::Split(step) => step.id == id,
            Self::Scatter(step) => step.id == id,
            Self::ChangeBlock(step) => match &step.change {
                Grief::Fall { .. } => false,
                Grief::Land(landing) => landing.falling == id,
                Grief::Graze { sheep, .. } => *sheep == id,
                Grief::Raid { rabbit, .. } => *rabbit == id,
                Grief::Berries { fox, .. } => *fox == id,
                Grief::Merge { silverfish, .. } | Grief::WakeFriends { silverfish, .. } => {
                    *silverfish == id
                }
                Grief::EndermanTake { enderman, .. } | Grief::EndermanPlace { enderman, .. } => {
                    *enderman == id
                }
                Grief::Douse { potion, .. } => *potion == id,
                Grief::Compost(run) => run.villager() == id,
                Grief::Statue { golem, .. } => *golem == id,
                Grief::WitherBreak { wither, .. } => *wither == id,
                Grief::RavagerBreak { ravager, .. } => *ravager == id,
                Grief::BeeGrow { bee, .. } => *bee == id,
                Grief::TurtleEgg { turtle, .. } => *turtle == id,
                Grief::Frogspawn { frog, .. } => *frog == id,
            },
            Self::Form(step) => step.entity == id,
            Self::Ignite(step) => step.projectile == id,
            Self::EnterHive { bee: mob, .. }
            | Self::EggBreak { mob, .. }
            | Self::DoorOpen { mob, .. }
            | Self::TargetBlock { mob, .. } => *mob == id,
            Self::EntityExplode(step) => step.discard == Some(id),
            Self::Splash(step) => step.potion == id,
            Self::Prime(step) => step.source == id,
            Self::Relocate(step) => step.entity == id,
            Self::ProjectileHit { arrow, target } => *arrow == id || *target == id,
            Self::Aim(step) => step.mob == id,
            Self::Breed(step) => step.parent == id || step.partner == id,
            Self::Love(step) => step.target == id,
            Self::Tame(step) => step.target == id,
            Self::MobRegain(step) => step.target == id,
            Self::PigAnger(step) => step.mob == id,
            Self::Dye(step) => step.target == id,
            Self::Shear(step) => step.target == id,
            Self::BucketEntity(step) => step.target == id,
            Self::InteractEntity(step) => step.target == id,
            _ => false,
        }
    }

    pub(crate) fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Velocity(one), Self::Velocity(other)) => one.player == other.player,
            (Self::PickupArrow(one), Self::PickupArrow(other)) => {
                one.player == other.player && one.arrow == other.arrow
            }
            (Self::Cook(one), Self::Cook(other)) => one.pos == other.pos && one.slot == other.slot,
            (Self::ChunkUnload(one), Self::ChunkUnload(other)) => one == other,
            (Self::Combust(one), Self::Combust(other)) => {
                one.target == other.target && one.kind == other.kind && one.igniter == other.igniter
            }
            (Self::ItemDespawn(one), Self::ItemDespawn(other)) => one == other,
            (Self::Split(one), Self::Split(other)) => one.id == other.id,
            (Self::ChangeBlock(one), Self::ChangeBlock(other)) => {
                match (&one.change, &other.change) {
                    (Grief::Fall { pos: one, .. }, Grief::Fall { pos: other, .. }) => one == other,
                    _ => false,
                }
            }
            (Self::Form(one), Self::Form(other)) => one.pos == other.pos,
            (Self::Ignite(one), Self::Ignite(other)) => one.pos == other.pos,
            (Self::EggBreak { mob: one, .. }, Self::EggBreak { mob: other, .. }) => one == other,
            (
                Self::DoorOpen {
                    mob: one_mob,
                    pos: one,
                },
                Self::DoorOpen {
                    mob: other_mob,
                    pos: other,
                },
            ) => one_mob == other_mob && one == other,
            (Self::TargetBlock { mob: one, .. }, Self::TargetBlock { mob: other, .. }) => {
                one == other
            }
            (Self::Prime(one), Self::Prime(other)) => one.source == other.source,
            (Self::Breed(one), Self::Breed(other)) => one.parent == other.parent,
            (Self::Aim(one), Self::Aim(other)) => {
                one.deferred && one.mob == other.mob && one.seeker == other.seeker
            }
            (
                Self::ProjectileHit { arrow, target },
                Self::ProjectileHit {
                    arrow: other_arrow,
                    target: other_target,
                },
            ) => arrow == other_arrow && target == other_target,
            (Self::PickupItem(one), Self::PickupItem(other)) => {
                one.player == other.player && one.item == other.item
            }
            (Self::Gather(one), Self::Gather(other)) => {
                one.fox == other.fox && one.item == other.item
            }
            (Self::MobPickup(one), Self::MobPickup(other)) => {
                one.collector == other.collector && one.item == other.item
            }
            (Self::PiglinPickup(one), Self::PiglinPickup(other)) => {
                one.collector == other.collector && one.item == other.item
            }
            (Self::Potion(one), Self::Potion(other)) => {
                matches!((one.potion, other.potion), (Potion::Expire(a), Potion::Expire(b)) if a.effect == b.effect)
                    && one.holder == other.holder
            }
            (Self::Hurt(one), Self::Hurt(other)) => {
                one.repeats() && one.victim == other.victim && one.source.kind == other.source.kind
            }
            _ => false,
        }
    }

    pub(crate) fn may_skip(&self) -> bool {
        match self {
            Self::Hurt(step) => step.repeats(),
            Self::Potion(step) => matches!(step.potion, Potion::Expire(_)),
            Self::Combust(_)
            | Self::Split(_)
            | Self::ItemDespawn(_)
            | Self::PickupItem(_)
            | Self::Gather(_)
            | Self::MobPickup(_)
            | Self::PiglinPickup(_) => true,
            Self::ChangeBlock(step) => matches!(step.change, Grief::Fall { .. }),
            Self::EggBreak { .. }
            | Self::DoorOpen { .. }
            | Self::TargetBlock { .. }
            | Self::Form(_)
            | Self::Ignite(_)
            | Self::Prime(_)
            | Self::ProjectileHit { .. } => true,
            Self::Aim(step) => step.deferred,
            Self::Breed(_) => true,
            _ => matches!(
                self,
                Self::Velocity(_) | Self::PickupArrow(_) | Self::Cook(_) | Self::ChunkUnload(_)
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Undo {
    SendBack(Vec3),
}
