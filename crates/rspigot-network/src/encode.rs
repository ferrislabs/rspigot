use std::borrow::Cow;

use bytes::Bytes;
use rspigot_core::commands::{
    ArgumentParser as CoreParser, CommandNodeKind as CoreNodeKind, CommandTree,
};
use rspigot_core::events::{
    Abilities, Animation, ClientEvent, EntityKind, EntityStatus, EquipmentSlot as CoreSlot,
    GameEvent as CoreGameEvent, MetadataValue, MoveDelta, Movement, PlayerListEntry,
    Pose as CorePose, PositionPath as CorePath, SoundSource as CoreSoundSource, SpawnInfo,
    StateKind, VariantKind,
};
use rspigot_data::variants::VariantFamily;
use rspigot_domain::chunk::{
    ChunkColumn, HeightmapKind as DomainHeightmap, LightArray as DomainLight,
};
use rspigot_domain::dye::dye_color_id;
use rspigot_domain::effect::{AttributeOperation, AttributeState, EffectInstance};
use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::entity::trading::{ItemCost, MerchantOffer};
use rspigot_domain::item::{CUSTOM_NAME, ENCHANTMENTS, ItemStack, LORE, MAX_LORE_LINES, Slot};
use rspigot_domain::particle::ParticleData;
use rspigot_domain::player::GameMode as DomainGameMode;
use rspigot_domain::world::Difficulty as DomainDifficulty;
use rspigot_domain::{BlockPos, ChunkPos, Rotation, Vec3};
use rspigot_protocol::codec::{Angle, Position};
use rspigot_protocol::frame::{Compression, build_frame};
use rspigot_protocol::play::chat::ChatTypeBound;
use rspigot_protocol::play::chunk::{
    BlockEntityInfo, ChunkSection as WireSection, Heightmap as WireHeightmap,
    HeightmapKind as WireHeightmap2, LevelChunkData, LightData, PaletteStrategy,
    PalettedContainer as WireContainer, write_sections,
};
use rspigot_protocol::play::clientbound as cb;
use rspigot_protocol::play::commands::{
    ArgumentParser as WireParser, CommandNode as WireNode, CommandNodeKind as WireNodeKind,
    SimpleParser, StringKind, Suggestion, parser,
};
use rspigot_protocol::play::effect::{FLAG_AMBIENT, FLAG_BLEND, FLAG_SHOW_ICON, FLAG_VISIBLE};
use rspigot_protocol::play::entity::{
    AttributeModifier as WireModifier, AttributeOperation as WireOperation, AttributeSnapshot,
    DeltaStep, EquipmentSlot as WireSlot, PositionPath, PositionStep, SwingAnimationStyle,
    SwingAnimationType, VecDelta,
};
use rspigot_protocol::play::inventory::{RecipePropertySet, SlotDisplay, StonecutterEntry};
use rspigot_protocol::play::metadata::{
    EntityDataItem, EntityDataValue, EnumState, RegistryVariant,
};
use rspigot_protocol::play::player::{CommonPlayerSpawnInfo, PlayerInfoActions, PlayerInfoEntry};
use rspigot_protocol::play::slot::{
    ComponentValue, DataComponentPatch, ItemStack as WireStack, ItemStackTemplate,
};
use rspigot_protocol::play::types::HolderSet;
use rspigot_protocol::play::types::{
    ByteBitSet, ChunkPos as WireChunkPos, Difficulty as WireDifficulty, Direction as WireDirection,
    GameMode as WireGameMode, GlobalPos, Hand as WireHand, Holder, LpVec3, Pose as WirePose,
    PositionMoveRotation, Relatives, SoundSource as WireSoundSource, Vec3 as WireVec3,
};
use rspigot_protocol::play::wire::write_var;
use rspigot_protocol::play::world::{ClockState, ClockUpdate, GameEventKind};
use rspigot_protocol::types::{ProfileProperty, Text};
use rspigot_protocol::{Encode, Packet, write_packet};

use crate::cache::ChunkPacketCache;
use crate::connection::{ConnectionError, PacketWriter};
use crate::text::to_nbt;

pub trait PacketSink {
    fn queue<P: Encode + ?Sized>(&mut self, id: i32, packet: &P) -> Result<(), ConnectionError>;
    fn queue_prebuilt(&mut self, frame: &[u8]);
}

impl PacketSink for PacketWriter {
    fn queue<P: Encode + ?Sized>(&mut self, id: i32, packet: &P) -> Result<(), ConnectionError> {
        PacketWriter::queue(self, id, packet)
    }

    fn queue_prebuilt(&mut self, frame: &[u8]) {
        PacketWriter::queue_prebuilt(self, frame);
    }
}

pub struct Encoder {
    online_mode: bool,
}

fn position(pos: BlockPos) -> Position {
    Position {
        x: pos.x,
        y: pos.y,
        z: pos.z,
    }
}

fn wire_vec(v: Vec3) -> WireVec3 {
    WireVec3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

fn text(component: &rspigot_domain::text::Component) -> Text {
    Text(to_nbt(component))
}

fn game_mode(mode: DomainGameMode) -> WireGameMode {
    match mode {
        DomainGameMode::Survival => WireGameMode::Survival,
        DomainGameMode::Creative => WireGameMode::Creative,
        DomainGameMode::Adventure => WireGameMode::Adventure,
        DomainGameMode::Spectator => WireGameMode::Spectator,
    }
}

fn difficulty(d: DomainDifficulty) -> WireDifficulty {
    match d {
        DomainDifficulty::Peaceful => WireDifficulty::Peaceful,
        DomainDifficulty::Easy => WireDifficulty::Easy,
        DomainDifficulty::Normal => WireDifficulty::Normal,
        DomainDifficulty::Hard => WireDifficulty::Hard,
    }
}

pub fn update_recipes() -> cb::UpdateRecipes {
    cb::UpdateRecipes {
        property_sets: rspigot_domain::recipes::property_sets()
            .iter()
            .map(|(key, items)| RecipePropertySet {
                key: (*key).to_owned(),
                items: items.iter().copied().map(i32::from).collect(),
            })
            .collect(),
        stonecutter: rspigot_domain::stonecutting::recipes()
            .iter()
            .map(|cut| StonecutterEntry {
                input: HolderSet::Ids(
                    cut.ingredient
                        .items()
                        .iter()
                        .map(|item| i32::from(item.0))
                        .collect(),
                ),
                display: SlotDisplay::ItemStack(ItemStackTemplate {
                    item: i32::from(cut.result.id().0),
                    count: i32::from(cut.count),
                    components: DataComponentPatch::EMPTY,
                }),
            })
            .collect(),
    }
}

fn component_type(name: &str) -> Option<i32> {
    rspigot_data::registries::static_index("minecraft:data_component_type", name)
        .and_then(|id| i32::try_from(id).ok())
}

fn optional_absent(out: &mut Vec<u8>) {
    false.encode(out);
}

const FIREWORK_SHAPES: [&str; 5] = ["small_ball", "large_ball", "star", "creeper", "burst"];

fn write_var_len(len: usize, out: &mut Vec<u8>) -> Option<()> {
    write_var(i32::try_from(len).ok()?, out);
    Some(())
}

fn write_colors(value: Option<&[rspigot_nbt::Tag]>, out: &mut Vec<u8>) -> Option<()> {
    let colors = value.unwrap_or_default();
    write_var_len(colors.len(), out)?;
    for color in colors {
        out.extend_from_slice(&i32::try_from(color.as_i64()?).ok()?.to_be_bytes());
    }
    Some(())
}

fn write_explosion(explosion: &rspigot_nbt::Compound, out: &mut Vec<u8>) -> Option<()> {
    let name = explosion.get_str("shape")?;
    let shape = FIREWORK_SHAPES.iter().position(|shape| *shape == name)?;
    write_var(i32::try_from(shape).ok()?, out);
    write_colors(explosion.get_list("colors"), out)?;
    write_colors(explosion.get_list("fade_colors"), out)?;
    explosion.get_bool("has_trail").unwrap_or(false).encode(out);
    explosion
        .get_bool("has_twinkle")
        .unwrap_or(false)
        .encode(out);
    Some(())
}

fn write_banner_layer(layer: &rspigot_nbt::Compound, out: &mut Vec<u8>) -> Option<()> {
    match layer.get("pattern")? {
        rspigot_nbt::Tag::String(name) => {
            let id = rspigot_data::registries::synced_index("minecraft:banner_pattern", name)?;
            write_var(i32::try_from(id).ok()?.checked_add(1)?, out);
        }
        rspigot_nbt::Tag::Compound(direct) => {
            write_var(0, out);
            direct.get_str("asset_id")?.encode(out);
            direct.get_str("translation_key")?.encode(out);
        }
        _ => return None,
    }
    write_var(
        i32::try_from(dye_color_id(layer.get_str("color")?)?).ok()?,
        out,
    );
    Some(())
}

fn write_pot_side(side: Option<&rspigot_nbt::Compound>, out: &mut Vec<u8>) -> Option<()> {
    let Some(template) = side else {
        false.encode(out);
        return Some(());
    };
    true.encode(out);
    let item = rspigot_data::registries::static_index("minecraft:item", template.get_str("id")?)?;
    write_var(i32::try_from(item).ok()?, out);
    write_var(template.get_i32("count").unwrap_or(1), out);
    let patch = DataComponentPatch {
        added: template
            .get_compound("components")
            .map(|components| {
                components
                    .iter()
                    .filter_map(|(name, value)| component_value(name, value))
                    .collect()
            })
            .unwrap_or_default(),
        removed: Vec::new(),
    };
    patch.encode_trusted(out);
    Some(())
}

fn component_value(name: &str, value: &rspigot_nbt::Tag) -> Option<ComponentValue> {
    let mut data = Vec::new();
    match name {
        "minecraft:damage" | "minecraft:ominous_bottle_amplifier" => {
            let value = i32::try_from(value.as_i64()?).ok()?;
            write_var(value, &mut data);
        }
        "minecraft:potion_contents" => {
            let potion = value.as_compound()?.get_str("potion");
            match potion.and_then(|p| rspigot_data::registries::static_index("minecraft:potion", p))
            {
                Some(id) => {
                    true.encode(&mut data);
                    write_var(i32::try_from(id).ok()?, &mut data);
                }
                None => optional_absent(&mut data),
            }
            optional_absent(&mut data);
            write_var(0, &mut data);
            optional_absent(&mut data);
        }
        "minecraft:trim" => {
            let trim = value.as_compound()?;
            for (key, registry) in [
                ("material", "minecraft:trim_material"),
                ("pattern", "minecraft:trim_pattern"),
            ] {
                let id = rspigot_data::registries::synced_index(registry, trim.get_str(key)?)?;
                write_var(i32::try_from(id).ok()?.checked_add(1)?, &mut data);
            }
        }
        "minecraft:dyed_color" => {
            data.extend_from_slice(&i32::try_from(value.as_i64()?).ok()?.to_be_bytes());
        }
        "minecraft:base_color" => {
            write_var(
                i32::try_from(dye_color_id(value.as_str()?)?).ok()?,
                &mut data,
            );
        }
        "minecraft:firework_explosion" => write_explosion(value.as_compound()?, &mut data)?,
        "minecraft:fireworks" => {
            let fireworks = value.as_compound()?;
            write_var(fireworks.get_i32("flight_duration").unwrap_or(0), &mut data);
            let explosions = fireworks.get_list("explosions").unwrap_or_default();
            write_var_len(explosions.len(), &mut data)?;
            for explosion in explosions {
                write_explosion(explosion.as_compound()?, &mut data)?;
            }
        }
        "minecraft:banner_patterns" => {
            let layers = value.as_list()?;
            write_var_len(layers.len(), &mut data)?;
            for layer in layers {
                write_banner_layer(layer.as_compound()?, &mut data)?;
            }
        }
        "minecraft:pot_decorations" => {
            let sides = value.as_compound()?;
            for side in ["back", "left", "right", "front"] {
                write_pot_side(sides.get_compound(side), &mut data)?;
            }
        }
        CUSTOM_NAME => text(&component_from_tag(value)?).encode(&mut data),
        LORE => {
            let lines = value.as_list()?;
            if lines.len() > MAX_LORE_LINES {
                return None;
            }
            write_var_len(lines.len(), &mut data)?;
            for line in lines {
                text(&component_from_tag(line)?).encode(&mut data);
            }
        }
        ENCHANTMENTS => {
            let enchantments = value.as_compound()?;
            let levels = enchantments.get_compound("levels").unwrap_or(enchantments);
            write_var_len(levels.len(), &mut data)?;
            for (name, level) in levels.iter() {
                let id = rspigot_data::registries::synced_index("minecraft:enchantment", name)?;
                write_var(i32::try_from(id).ok()?, &mut data);
                write_var(i32::try_from(level.as_i64()?).ok()?, &mut data);
            }
        }
        "minecraft:suspicious_stew_effects" => {
            let entries = value.as_list()?;
            write_var(i32::try_from(entries.len()).ok()?, &mut data);
            for entry in entries {
                let entry = entry.as_compound()?;
                let effect = rspigot_data::registries::static_index(
                    "minecraft:mob_effect",
                    entry.get_str("id")?,
                )?;
                write_var(i32::try_from(effect).ok()?, &mut data);
                write_var(entry.get_i32("duration")?, &mut data);
            }
        }
        _ => return None,
    }
    Some(ComponentValue {
        type_id: component_type(name)?,
        data,
    })
}

fn trade_cost(cost: &ItemCost) -> cb::TradeCost {
    cb::TradeCost {
        item: i32::from(cost.item.0),
        count: cost.count,
        components: cost
            .components
            .iter()
            .filter_map(|(name, value)| component_value(name, value))
            .collect(),
    }
}

fn trade_offer(offer: &MerchantOffer) -> cb::TradeOffer {
    cb::TradeOffer {
        cost_a: trade_cost(&offer.cost_a),
        result: stack(&Some(offer.result.clone())),
        cost_b: offer.cost_b.as_ref().map(trade_cost),
        out_of_stock: offer.is_out_of_stock(),
        uses: offer.uses,
        max_uses: offer.max_uses,
        xp: offer.xp,
        special_price: offer.special_price_diff,
        price_multiplier: offer.price_multiplier,
        demand: offer.demand,
    }
}

pub fn stack(slot: &Slot) -> WireStack {
    match slot {
        Some(ItemStack {
            item,
            count,
            components,
        }) => {
            let mut wire = WireStack::new(i32::from(item.0), i32::from(count.get()));
            wire.components.added = components
                .iter()
                .filter_map(|(name, value)| component_value(name, value))
                .collect();
            wire
        }
        None => WireStack::EMPTY,
    }
}

fn spawn_info(spawn: &SpawnInfo) -> CommonPlayerSpawnInfo {
    CommonPlayerSpawnInfo {
        dimension_type: rspigot_data::registries::dimension_type_id(
            spawn.dimension.dimension_type.as_str(),
        )
        .map_or(0, |id| id as i32),
        dimension: spawn.dimension.key.to_string(),
        seed: spawn.hashed_seed,
        game_mode: game_mode(spawn.game_mode),
        previous_game_mode: spawn.previous_game_mode.map(game_mode),
        is_debug: spawn.is_debug,
        is_flat: spawn.is_flat,
        last_death_location: spawn
            .death_location
            .as_ref()
            .map(|(dimension, pos)| GlobalPos {
                dimension: dimension.to_string(),
                pos: position(*pos),
            }),
        portal_cooldown: spawn.portal_cooldown,
        sea_level: spawn.dimension.sea_level,
    }
}

fn info_entry(entry: &PlayerListEntry) -> PlayerInfoEntry {
    PlayerInfoEntry {
        profile_id: entry.profile.id.0,
        name: entry.profile.name.clone(),
        properties: entry
            .profile
            .properties
            .iter()
            .map(|p| ProfileProperty {
                name: p.name.clone(),
                value: p.value.clone(),
                signature: p.signature.clone(),
            })
            .collect(),
        chat_session: None,
        game_mode: game_mode(entry.game_mode),
        listed: entry.listed,
        latency: entry.latency_ms,
        display_name: entry.display_name.as_ref().map(text),
        list_order: 0,
        show_hat: true,
    }
}

fn abilities(a: &Abilities) -> cb::PlayerAbilities {
    cb::PlayerAbilities {
        invulnerable: a.invulnerable,
        flying: a.flying,
        can_fly: a.may_fly,
        instabuild: a.instant_build,
        flying_speed: a.fly_speed,
        walking_speed: a.walk_speed,
    }
}

fn entity_type(kind: EntityKind) -> i32 {
    let name = match kind {
        EntityKind::Player => "minecraft:player",
        EntityKind::Item => "minecraft:item",
        EntityKind::FallingBlock => "minecraft:falling_block",
        EntityKind::Other(id) => return i32::from(id),
    };
    rspigot_data::registries::static_index("minecraft:entity_type", name).map_or(0, |id| id as i32)
}

fn sound_source(source: CoreSoundSource) -> WireSoundSource {
    match source {
        CoreSoundSource::Master => WireSoundSource::Master,
        CoreSoundSource::Music => WireSoundSource::Music,
        CoreSoundSource::Records => WireSoundSource::Records,
        CoreSoundSource::Weather => WireSoundSource::Weather,
        CoreSoundSource::Blocks => WireSoundSource::Blocks,
        CoreSoundSource::Hostile => WireSoundSource::Hostile,
        CoreSoundSource::Neutral => WireSoundSource::Neutral,
        CoreSoundSource::Players => WireSoundSource::Players,
        CoreSoundSource::Ambient => WireSoundSource::Ambient,
        CoreSoundSource::Voice => WireSoundSource::Voice,
        CoreSoundSource::Ui => WireSoundSource::Ui,
    }
}

fn equipment_break_event(slot: CoreSlot) -> i8 {
    match slot {
        CoreSlot::MainHand => 47,
        CoreSlot::OffHand => 48,
        CoreSlot::Head => 49,
        CoreSlot::Chest => 50,
        CoreSlot::Legs => 51,
        CoreSlot::Feet => 52,
        CoreSlot::Body => 65,
        CoreSlot::Saddle => 68,
    }
}

fn direction_of(index: u8) -> WireDirection {
    match index {
        0 => WireDirection::Down,
        1 => WireDirection::Up,
        2 => WireDirection::North,
        3 => WireDirection::South,
        4 => WireDirection::West,
        _ => WireDirection::East,
    }
}

fn pose(p: CorePose) -> WirePose {
    match p {
        CorePose::Standing => WirePose::Standing,
        CorePose::FallFlying => WirePose::FallFlying,
        CorePose::Sleeping => WirePose::Sleeping,
        CorePose::Swimming => WirePose::Swimming,
        CorePose::SpinAttack => WirePose::SpinAttack,
        CorePose::Crouching => WirePose::Crouching,
        CorePose::LongJumping => WirePose::LongJumping,
        CorePose::Dying => WirePose::Dying,
        CorePose::Croaking => WirePose::Croaking,
        CorePose::UsingTongue => WirePose::UsingTongue,
        CorePose::Sitting => WirePose::Sitting,
        CorePose::Roaring => WirePose::Roaring,
        CorePose::Sniffing => WirePose::Sniffing,
        CorePose::Emerging => WirePose::Emerging,
        CorePose::Digging => WirePose::Digging,
        CorePose::Sliding => WirePose::Sliding,
        CorePose::Shooting => WirePose::Shooting,
        CorePose::Inhaling => WirePose::Inhaling,
    }
}

fn equipment_slot(slot: CoreSlot) -> WireSlot {
    match slot {
        CoreSlot::MainHand => WireSlot::MainHand,
        CoreSlot::OffHand => WireSlot::OffHand,
        CoreSlot::Feet => WireSlot::Feet,
        CoreSlot::Legs => WireSlot::Legs,
        CoreSlot::Chest => WireSlot::Chest,
        CoreSlot::Head => WireSlot::Head,
        CoreSlot::Body => WireSlot::Body,
        CoreSlot::Saddle => WireSlot::Saddle,
    }
}

fn simple(id: i32) -> WireParser {
    SimpleParser::new(id).map_or(
        WireParser::String(StringKind::SingleWord),
        WireParser::Simple,
    )
}

fn parser(p: &CoreParser) -> WireParser {
    match p {
        CoreParser::Bool => simple(parser::BOOL),
        CoreParser::Integer { min, max } => WireParser::Integer {
            min: *min,
            max: *max,
        },
        CoreParser::Float { min, max } => WireParser::Float {
            min: *min,
            max: *max,
        },
        CoreParser::Double { min, max } => WireParser::Double {
            min: *min,
            max: *max,
        },
        CoreParser::Word => WireParser::String(StringKind::SingleWord),
        CoreParser::QuotedString => WireParser::String(StringKind::QuotablePhrase),
        CoreParser::GreedyString => WireParser::String(StringKind::GreedyPhrase),
        CoreParser::Entity {
            single,
            players_only,
        } => WireParser::Entity {
            single: *single,
            players_only: *players_only,
        },
        CoreParser::GameProfile => simple(parser::GAME_PROFILE),
        CoreParser::BlockPos => simple(parser::BLOCK_POS),
        CoreParser::Vec3 => simple(parser::VEC3),
        CoreParser::BlockState => simple(parser::BLOCK_STATE),
        CoreParser::ItemStack => simple(parser::ITEM_STACK),
        CoreParser::Message => simple(parser::MESSAGE),
        CoreParser::Time { min } => WireParser::Time { min: *min },
        CoreParser::GameMode => simple(parser::GAMEMODE),
        CoreParser::Dimension => simple(parser::DIMENSION),
        CoreParser::ResourceLocation => simple(parser::RESOURCE_LOCATION),
        CoreParser::Resource { registry } => WireParser::Resource {
            registry: registry.clone(),
        },
        CoreParser::NbtCompoundTag => simple(parser::NBT_COMPOUND_TAG),
    }
}

fn commands(tree: &CommandTree) -> cb::Commands {
    cb::Commands {
        nodes: tree
            .nodes
            .iter()
            .map(|node| WireNode {
                kind: match &node.kind {
                    CoreNodeKind::Root => WireNodeKind::Root,
                    CoreNodeKind::Literal(name) => WireNodeKind::Literal { name: name.clone() },
                    CoreNodeKind::Argument {
                        name,
                        parser: p,
                        suggests,
                    } => WireNodeKind::Argument {
                        name: name.clone(),
                        parser: parser(p),
                        suggestions: suggests.clone(),
                    },
                },
                executable: node.executable,
                restricted: false,
                children: node.children.iter().map(|c| *c as i32).collect(),
                redirect: node.redirect.map(|r| r as i32),
            })
            .collect(),
        root_index: tree.root as i32,
    }
}

fn enum_state(kind: StateKind) -> EnumState {
    match kind {
        StateKind::Sniffer => EnumState::Sniffer,
        StateKind::Armadillo => EnumState::Armadillo,
        StateKind::CopperGolem => EnumState::CopperGolem,
        StateKind::WeatheringCopper => EnumState::WeatheringCopper,
    }
}

fn registry_variant(kind: VariantKind) -> RegistryVariant {
    match kind {
        VariantKind::Mob(VariantFamily::Cow) => RegistryVariant::Cow,
        VariantKind::Sound(VariantFamily::Cow) => RegistryVariant::CowSound,
        VariantKind::Mob(VariantFamily::Pig) => RegistryVariant::Pig,
        VariantKind::Sound(VariantFamily::Pig) => RegistryVariant::PigSound,
        VariantKind::Mob(VariantFamily::Chicken) => RegistryVariant::Chicken,
        VariantKind::Sound(VariantFamily::Chicken) => RegistryVariant::ChickenSound,
        VariantKind::Mob(VariantFamily::Wolf) => RegistryVariant::Wolf,
        VariantKind::Sound(VariantFamily::Wolf) => RegistryVariant::WolfSound,
        VariantKind::Mob(VariantFamily::Cat) => RegistryVariant::Cat,
        VariantKind::Sound(VariantFamily::Cat) => RegistryVariant::CatSound,
    }
}

fn particle_options(name: &str, options: &ParticleData) -> Option<cb::ParticleOptions> {
    let kind = rspigot_data::registries::static_index("minecraft:particle_type", name)
        .and_then(|id| i32::try_from(id).ok())?;
    let mut data = Vec::new();
    match options {
        ParticleData::None => {}
        ParticleData::Item(item) => ItemStackTemplate {
            item: i32::from(item.0),
            count: 1,
            components: DataComponentPatch::EMPTY,
        }
        .encode(&mut data),
        ParticleData::Block(state) => write_var(i32::from(state.0), &mut data),
        ParticleData::Dust { color, scale } => {
            data.extend_from_slice(&color.to_be_bytes());
            data.extend_from_slice(&scale.to_be_bytes());
        }
    }
    Some(cb::ParticleOptions { kind, data })
}

fn game_event(event: CoreGameEvent) -> cb::GameEvent {
    let (kind, param) = match event {
        CoreGameEvent::NoRespawnBlockAvailable => (GameEventKind::NoRespawnBlockAvailable, 0.0),
        CoreGameEvent::StartRaining => (GameEventKind::StartRaining, 0.0),
        CoreGameEvent::StopRaining => (GameEventKind::StopRaining, 0.0),
        CoreGameEvent::ChangeGameMode(mode) => {
            (GameEventKind::ChangeGameMode, f32::from(mode.id()))
        }
        CoreGameEvent::WinGame => (GameEventKind::WinGame, 0.0),
        CoreGameEvent::DemoEvent => (GameEventKind::DemoEvent, 0.0),
        CoreGameEvent::ArrowHitPlayer => (GameEventKind::PlayArrowHitSound, 0.0),
        CoreGameEvent::RainLevelChange(v) => (GameEventKind::RainLevelChange, v),
        CoreGameEvent::ThunderLevelChange(v) => (GameEventKind::ThunderLevelChange, v),
        CoreGameEvent::PufferFishSting => (GameEventKind::PufferFishSting, 0.0),
        CoreGameEvent::GuardianElderEffect => (GameEventKind::GuardianElderEffect, 0.0),
        CoreGameEvent::ImmediateRespawn(on) => {
            (GameEventKind::ImmediateRespawn, if on { 1.0 } else { 0.0 })
        }
        CoreGameEvent::LimitedCrafting(on) => {
            (GameEventKind::LimitedCrafting, if on { 1.0 } else { 0.0 })
        }
        CoreGameEvent::LevelChunksLoadStart => (GameEventKind::LevelChunksLoadStart, 0.0),
    };
    cb::GameEvent { event: kind, param }
}

fn fluid_count(section: &rspigot_domain::chunk::ChunkSection) -> i16 {
    let blocks = section.blocks();
    let fluid_slots: Vec<bool> = blocks
        .palette()
        .iter()
        .map(|state| state.flags().has_fluid())
        .collect();
    if !fluid_slots.iter().any(|f| *f) {
        return 0;
    }
    if blocks.single_value().is_some() {
        return 4096;
    }
    (0..4096)
        .filter(|i| fluid_slots[blocks.palette_index(*i)])
        .count() as i16
}

fn container_for<T: Copy + Eq, S: rspigot_domain::chunk::PaletteStrategy>(
    container: &rspigot_domain::chunk::PalettedContainer<T, S>,
    strategy: &PaletteStrategy,
    id: impl Fn(T) -> u32,
    palette: &mut Vec<u32>,
) -> WireContainer<'static> {
    if let Some(value) = container.single_value() {
        return WireContainer::single(id(value));
    }
    let bits = container.bits();
    if bits <= strategy.max_indirect_bits && bits >= strategy.min_indirect_bits {
        palette.clear();
        palette.extend(container.palette().iter().map(|v| id(*v)));
        return WireContainer::Indirect {
            bits,
            palette: Cow::Owned(std::mem::take(palette)),
            packed: Cow::Owned(container.packed().to_vec()),
        };
    }
    let entries: Vec<u32> = container.values().map(id).collect();
    WireContainer::from_entries(strategy, &entries)
}

fn light_data(light: &[Option<DomainLight>]) -> (ByteBitSet, ByteBitSet, Vec<Cow<'_, [u8; 2048]>>) {
    let mut present = Vec::new();
    let mut empty = Vec::new();
    let mut arrays = Vec::new();
    for (index, layer) in light.iter().enumerate() {
        match layer {
            Some(array) if !array.is_uniform(0) => {
                present.push(index);
                arrays.push(Cow::Borrowed(array.as_bytes()));
            }
            _ => empty.push(index),
        }
    }
    (
        ByteBitSet::from_bits(present),
        ByteBitSet::from_bits(empty),
        arrays,
    )
}

fn heightmap_kind(kind: DomainHeightmap) -> WireHeightmap2 {
    match kind {
        DomainHeightmap::WorldSurfaceWg => WireHeightmap2::WorldSurfaceWorldgen,
        DomainHeightmap::WorldSurface => WireHeightmap2::WorldSurface,
        DomainHeightmap::OceanFloorWg => WireHeightmap2::OceanFloorWorldgen,
        DomainHeightmap::OceanFloor => WireHeightmap2::OceanFloor,
        DomainHeightmap::MotionBlocking => WireHeightmap2::MotionBlocking,
        DomainHeightmap::MotionBlockingNoLeaves => WireHeightmap2::MotionBlockingNoLeaves,
    }
}

pub fn chunk_packet(chunk: &ChunkColumn, out: &mut Vec<u8>) {
    let blocks = PaletteStrategy::blocks(rspigot_blocks::STATE_COUNT as u32);
    let biome_count = rspigot_data::registries::synced_entries("minecraft:worldgen/biome")
        .map_or(1, <[&str]>::len) as u32;
    let biomes = PaletteStrategy::biomes(biome_count);
    let mut scratch = Vec::new();
    let sections: Vec<WireSection<'static>> = chunk
        .sections()
        .iter()
        .map(|section| WireSection {
            non_empty_block_count: section.non_air_count() as i16,
            fluid_count: fluid_count(section),
            blocks: container_for(section.blocks(), &blocks, |s| u32::from(s.0), &mut scratch),
            biomes: container_for(section.biomes(), &biomes, |b| u32::from(b.0), &mut scratch),
        })
        .collect();
    let mut data = Vec::with_capacity(16 * 1024);
    write_sections(&sections, &mut data);
    let height = chunk.height().height;
    let heightmaps = chunk
        .heightmaps
        .maps
        .iter()
        .filter(|(kind, _)| DomainHeightmap::CLIENT.contains(kind))
        .map(|(kind, map)| WireHeightmap {
            kind: heightmap_kind(*kind),
            data: Cow::Owned(map.pack(height).into_iter().map(|w| w as i64).collect()),
        })
        .collect();
    let block_entities: Vec<BlockEntityInfo> = chunk
        .block_entities
        .iter()
        .filter_map(|entity| {
            let kind = rspigot_data::registries::static_index(
                "minecraft:block_entity_type",
                entity.kind.as_str(),
            )?;
            Some(BlockEntityInfo {
                packed_xz: (((entity.pos.x & 15) << 4) | (entity.pos.z & 15)) as u8,
                y: entity.pos.y as i16,
                kind: kind as i32,
                data: Some(rspigot_nbt::Tag::Compound(entity.data.clone())),
            })
        })
        .collect();
    let (sky_y_mask, empty_sky_y_mask, sky_updates) = light_data(&chunk.light.sky);
    let (block_y_mask, empty_block_y_mask, block_updates) = light_data(&chunk.light.block);
    let packet = cb::LevelChunkWithLight {
        x: chunk.pos().x,
        z: chunk.pos().z,
        chunk: LevelChunkData {
            heightmaps,
            data: Cow::Owned(data),
            block_entities: Cow::Owned(block_entities),
        },
        light: LightData {
            sky_y_mask,
            block_y_mask,
            empty_sky_y_mask,
            empty_block_y_mask,
            sky_updates,
            block_updates,
        },
    };
    write_packet(&packet, out);
}

fn effect_flags(effect: &EffectInstance, blend: bool) -> u8 {
    let mut flags = 0;
    if effect.ambient {
        flags |= FLAG_AMBIENT;
    }
    if effect.visible {
        flags |= FLAG_VISIBLE;
    }
    if effect.show_icon {
        flags |= FLAG_SHOW_ICON;
    }
    if blend {
        flags |= FLAG_BLEND;
    }
    flags
}

fn attribute_snapshot(state: &AttributeState) -> Option<AttributeSnapshot> {
    let attribute = rspigot_data::registries::static_index("minecraft:attribute", state.attribute)?;
    Some(AttributeSnapshot {
        attribute: attribute as i32,
        base: state.base,
        modifiers: state
            .modifiers
            .iter()
            .map(|m| WireModifier {
                id: m.id.to_string(),
                amount: m.amount,
                operation: match m.operation {
                    AttributeOperation::AddValue => WireOperation::AddValue,
                    AttributeOperation::AddMultipliedBase => WireOperation::AddMultipliedBase,
                    AttributeOperation::AddMultipliedTotal => WireOperation::AddMultipliedTotal,
                },
            })
            .collect(),
    })
}

fn wire_delta(delta: &MoveDelta) -> VecDelta {
    match delta {
        MoveDelta::Linear { xa, ya, za } => VecDelta::Linear {
            xa: *xa,
            ya: *ya,
            za: *za,
        },
        MoveDelta::Stepped(steps) => VecDelta::Stepped(
            steps
                .iter()
                .map(|step| DeltaStep {
                    ticks: step.ticks,
                    xa: step.xa,
                    ya: step.ya,
                    za: step.za,
                })
                .collect(),
        ),
    }
}

fn fixed(value: f64) -> i64 {
    (value * 4096.0).round() as i64
}

fn delta(from: Vec3, to: Vec3) -> Option<VecDelta> {
    let dx = i16::try_from(fixed(to.x) - fixed(from.x)).ok()?;
    let dy = i16::try_from(fixed(to.y) - fixed(from.y)).ok()?;
    let dz = i16::try_from(fixed(to.z) - fixed(from.z)).ok()?;
    Some(VecDelta::Linear {
        xa: dx,
        ya: dy,
        za: dz,
    })
}

impl Encoder {
    pub fn new(online_mode: bool) -> Self {
        Self { online_mode }
    }

    fn queue<P: Packet, W: PacketSink>(writer: &mut W, packet: &P) -> Result<(), ConnectionError> {
        writer.queue(P::ID, packet)
    }

    fn chunk<W: PacketSink>(
        writer: &mut W,
        dimension: &rspigot_domain::Identifier,
        chunk: &ChunkColumn,
        cache: &ChunkPacketCache,
        compression: Option<Compression>,
    ) -> Result<(), ConnectionError> {
        let key = (dimension.clone(), chunk.pos(), chunk.revision());
        let frame = cache.get_or_build(key, || {
            let mut body = Vec::with_capacity(32 * 1024);
            chunk_packet(chunk, &mut body);
            let mut frame = Vec::with_capacity(body.len() / 2 + 8);
            build_frame(&body, compression, &mut frame).map(|()| Bytes::from(frame))
        })?;
        writer.queue_prebuilt(&frame);
        Ok(())
    }

    pub fn encode<W: PacketSink>(
        &mut self,
        event: &ClientEvent,
        writer: &mut W,
        cache: &ChunkPacketCache,
        threshold: Option<usize>,
        level: u32,
    ) -> Result<(), ConnectionError> {
        match event {
            ClientEvent::Chunk { dimension, chunk } => {
                let compression = threshold.map(|threshold| Compression { threshold, level });
                Self::chunk(writer, dimension, chunk, cache, compression)
            }
            ClientEvent::Join(info) => Self::queue(
                writer,
                &cb::Login {
                    player_id: info.entity_id.0,
                    hardcore: info.hardcore,
                    levels: info
                        .dimension_names
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    max_players: info.max_players as i32,
                    chunk_radius: i32::from(info.view_distance),
                    simulation_distance: i32::from(info.simulation_distance),
                    reduced_debug_info: info.reduced_debug_info,
                    show_death_screen: info.enable_respawn_screen,
                    do_limited_crafting: info.do_limited_crafting,
                    spawn_info: spawn_info(&info.spawn),
                    online_mode: self.online_mode,
                    enforces_secure_chat: info.enforces_secure_chat,
                },
            ),
            ClientEvent::Respawn { spawn, keep_data } => Self::queue(
                writer,
                &cb::Respawn {
                    spawn_info: spawn_info(spawn),
                    data_to_keep: *keep_data,
                },
            ),
            ClientEvent::Difficulty {
                difficulty: d,
                locked,
            } => Self::queue(
                writer,
                &cb::ChangeDifficulty {
                    difficulty: difficulty(*d),
                    locked: *locked,
                },
            ),
            ClientEvent::Abilities(a) => Self::queue(writer, &abilities(a)),
            ClientEvent::RecipeSync => Self::queue(writer, &update_recipes()),
            ClientEvent::HeldSlot(slot) => Self::queue(
                writer,
                &cb::SetHeldSlot {
                    slot: i32::from(*slot),
                },
            ),
            ClientEvent::SpawnPosition {
                dimension,
                pos,
                yaw,
                pitch,
            } => Self::queue(
                writer,
                &cb::SetDefaultSpawnPosition {
                    pos: GlobalPos {
                        dimension: dimension.to_string(),
                        pos: position(*pos),
                    },
                    yaw: *yaw,
                    pitch: *pitch,
                },
            ),
            ClientEvent::Teleport {
                id,
                position: target,
                rotation,
            } => Self::queue(
                writer,
                &cb::PlayerPosition {
                    teleport_id: *id,
                    change: PositionMoveRotation {
                        position: wire_vec(*target),
                        delta: WireVec3::ZERO,
                        y_rot: rotation.yaw,
                        x_rot: rotation.pitch,
                    },
                    relatives: Relatives::NONE,
                },
            ),
            ClientEvent::ChunkBatchStart => Self::queue(writer, &cb::ChunkBatchStart),
            ClientEvent::ChunkBatchFinished { count } => Self::queue(
                writer,
                &cb::ChunkBatchFinished {
                    batch_size: *count as i32,
                },
            ),
            ClientEvent::ForgetChunk(pos) => Self::queue(
                writer,
                &cb::ForgetLevelChunk {
                    pos: WireChunkPos { x: pos.x, z: pos.z },
                },
            ),
            ClientEvent::CenterChunk(ChunkPos { x, z }) => {
                Self::queue(writer, &cb::SetChunkCacheCenter { x: *x, z: *z })
            }
            ClientEvent::ViewDistance(radius) => Self::queue(
                writer,
                &cb::SetChunkCacheRadius {
                    radius: i32::from(*radius),
                },
            ),
            ClientEvent::SimulationDistance(distance) => Self::queue(
                writer,
                &cb::SetSimulationDistance {
                    simulation_distance: i32::from(*distance),
                },
            ),
            ClientEvent::BlockUpdate { pos, state } => Self::queue(
                writer,
                &cb::BlockUpdate {
                    pos: position(*pos),
                    state: i32::from(state.0),
                },
            ),
            ClientEvent::BlockEntityUpdate { pos, kind, data } => {
                let id = rspigot_data::registries::static_index(
                    "minecraft:block_entity_type",
                    &format!("minecraft:{kind}"),
                )
                .and_then(|id| i32::try_from(id).ok());
                if let Some(id) = id {
                    Self::queue(
                        writer,
                        &cb::BlockEntityData {
                            pos: position(*pos),
                            kind: id,
                            data: rspigot_nbt::Tag::Compound(data.clone()),
                        },
                    )
                } else {
                    Ok(())
                }
            }
            ClientEvent::BlockEvent {
                pos,
                action,
                data,
                block,
            } => Self::queue(
                writer,
                &cb::BlockEvent {
                    pos: position(*pos),
                    action: *action,
                    data: *data,
                    block: i32::from(*block),
                },
            ),
            ClientEvent::BlockChangedAck(sequence) => Self::queue(
                writer,
                &cb::BlockChangedAck {
                    sequence: *sequence,
                },
            ),
            ClientEvent::BlockDestruction { entity, pos, stage } => Self::queue(
                writer,
                &cb::BlockDestruction {
                    id: entity.0,
                    pos: position(*pos),
                    progress: *stage as u8,
                },
            ),
            ClientEvent::GameEvent(event) => Self::queue(writer, &game_event(*event)),
            ClientEvent::Time {
                game_time,
                day_time,
                advance_time,
            } => {
                let clock = rspigot_data::registries::synced_index(
                    "minecraft:world_clock",
                    "minecraft:overworld",
                )
                .map_or(0, |id| id as i32);
                Self::queue(
                    writer,
                    &cb::SetTime {
                        game_time: *game_time,
                        clocks: vec![ClockUpdate {
                            clock,
                            state: ClockState {
                                total_ticks: *day_time,
                                partial_tick: 0.0,
                                rate: if *advance_time { 1.0 } else { 0.0 },
                            },
                        }],
                    },
                )
            }
            ClientEvent::SystemChat { message, overlay } => Self::queue(
                writer,
                &cb::SystemChat {
                    content: text(message),
                    overlay: *overlay,
                },
            ),
            ClientEvent::TitleText(message) => Self::queue(
                writer,
                &cb::SetTitleText {
                    text: text(message),
                },
            ),
            ClientEvent::SubtitleText(message) => Self::queue(
                writer,
                &cb::SetSubtitleText {
                    text: text(message),
                },
            ),
            ClientEvent::TitleTimes {
                fade_in,
                stay,
                fade_out,
            } => Self::queue(
                writer,
                &cb::SetTitlesAnimation {
                    fade_in: *fade_in,
                    stay: *stay,
                    fade_out: *fade_out,
                },
            ),
            ClientEvent::ClearTitles { reset } => Self::queue(
                writer,
                &cb::ClearTitles {
                    reset_times: *reset,
                },
            ),
            ClientEvent::PlayerChat {
                sender_name,
                message,
                ..
            } => {
                let chat =
                    rspigot_data::registries::synced_index("minecraft:chat_type", "minecraft:chat")
                        .map_or(0, |id| id as i32);
                Self::queue(
                    writer,
                    &cb::DisguisedChat {
                        message: Text(rspigot_nbt::Tag::String(message.clone())),
                        chat_type: ChatTypeBound {
                            chat_type: Holder::Reference(chat),
                            name: text(sender_name),
                            target_name: None,
                        },
                    },
                )
            }
            ClientEvent::PlayerInfo(entries) => Self::queue(
                writer,
                &cb::PlayerInfoUpdate {
                    actions: PlayerInfoActions::ADD_PLAYER
                        .union(PlayerInfoActions::UPDATE_GAME_MODE)
                        .union(PlayerInfoActions::UPDATE_LISTED)
                        .union(PlayerInfoActions::UPDATE_LATENCY)
                        .union(PlayerInfoActions::UPDATE_HAT),
                    entries: entries.iter().map(info_entry).collect(),
                },
            ),
            ClientEvent::PlayerInfoGameMode {
                player,
                game_mode: mode,
            } => Self::queue(
                writer,
                &cb::PlayerInfoUpdate {
                    actions: PlayerInfoActions::UPDATE_GAME_MODE,
                    entries: vec![PlayerInfoEntry {
                        profile_id: player.0,
                        name: String::new(),
                        properties: Vec::new(),
                        chat_session: None,
                        game_mode: game_mode(*mode),
                        listed: true,
                        latency: 0,
                        display_name: None,
                        list_order: 0,
                        show_hat: true,
                    }],
                },
            ),
            ClientEvent::PlayerInfoRemove(ids) => Self::queue(
                writer,
                &cb::PlayerInfoRemove {
                    profile_ids: ids.iter().map(|id| id.0).collect(),
                },
            ),
            ClientEvent::TabList { header, footer } => Self::queue(
                writer,
                &cb::TabList {
                    header: text(header),
                    footer: text(footer),
                },
            ),
            ClientEvent::SpawnEntity {
                entity,
                uuid,
                kind,
                position: at,
                rotation,
                head_yaw,
                data,
                velocity,
            } => Self::queue(
                writer,
                &cb::AddEntity {
                    entity_id: entity.0,
                    uuid: *uuid,
                    kind: entity_type(*kind),
                    x: at.x,
                    y: at.y,
                    z: at.z,
                    movement: LpVec3(wire_vec(*velocity)),
                    x_rot: Angle::from_degrees(rotation.pitch),
                    y_rot: Angle::from_degrees(rotation.yaw),
                    y_head_rot: Angle::from_degrees(*head_yaw),
                    data: *data,
                },
            ),
            ClientEvent::EntityMoved {
                entity,
                from,
                to,
                rotation,
                on_ground,
            } => Self::entity_moved(writer, entity.0, *from, *to, *rotation, *on_ground),
            ClientEvent::EntityMove { entity, movement } => {
                Self::entity_move(writer, entity.0, movement)
            }
            ClientEvent::BundleDelimiter => Self::queue(writer, &cb::BundleDelimiter),
            ClientEvent::EntityVelocity { entity, velocity } => Self::queue(
                writer,
                &cb::SetEntityMotion {
                    entity_id: entity.0,
                    movement: LpVec3(wire_vec(*velocity)),
                },
            ),
            ClientEvent::EntityHeadRotation { entity, yaw } => Self::queue(
                writer,
                &cb::RotateHead {
                    entity_id: entity.0,
                    y_head_rot: Angle::from_degrees(*yaw),
                },
            ),
            ClientEvent::EntityAnimation { entity, animation } => match animation {
                Animation::SwingMainArm | Animation::SwingOffHand => Self::queue(
                    writer,
                    &cb::SwingAnimation {
                        entity_id: entity.0,
                        hand: if *animation == Animation::SwingMainArm {
                            WireHand::MainHand
                        } else {
                            WireHand::OffHand
                        },
                        animation: SwingAnimationStyle {
                            kind: SwingAnimationType::Whack,
                            duration: 6,
                        },
                    },
                ),
                other => Self::queue(
                    writer,
                    &cb::Animate {
                        entity_id: entity.0,
                        action: match other {
                            Animation::WakeUp => 0,
                            Animation::CriticalHit => 1,
                            _ => 2,
                        },
                    },
                ),
            },
            ClientEvent::EntityMetadata { entity, entries } => Self::queue(
                writer,
                &cb::SetEntityData {
                    entity_id: entity.0,
                    items: entries
                        .iter()
                        .map(|entry| EntityDataItem {
                            index: entry.index,
                            value: match &entry.value {
                                MetadataValue::Byte(v) => EntityDataValue::Byte(*v),
                                MetadataValue::VarInt(v) => EntityDataValue::Int(*v),
                                MetadataValue::Float(v) => EntityDataValue::Float(*v),
                                MetadataValue::Pose(p) => EntityDataValue::Pose(pose(*p)),
                                MetadataValue::OptionalComponent(c) => {
                                    EntityDataValue::OptionalComponent(c.as_ref().map(text))
                                }
                                MetadataValue::Boolean(b) => EntityDataValue::Boolean(*b),
                                MetadataValue::Item(item) => {
                                    EntityDataValue::ItemStack(stack(item))
                                }
                                MetadataValue::BlockPos(pos) => {
                                    EntityDataValue::BlockPos(position(*pos))
                                }
                                MetadataValue::OptionalBlockPos(pos) => {
                                    EntityDataValue::OptionalBlockPos(pos.map(position))
                                }
                                MetadataValue::Variant(kind, id) => EntityDataValue::Variant {
                                    kind: registry_variant(*kind),
                                    id: *id as i32,
                                },
                                MetadataValue::Long(v) => EntityDataValue::Long(*v),
                                MetadataValue::Direction(face) => {
                                    EntityDataValue::Direction(direction_of(*face))
                                }
                                MetadataValue::State(kind, id) => EntityDataValue::State {
                                    kind: enum_state(*kind),
                                    id: *id,
                                },
                                MetadataValue::OptionalUnsignedInt(v) => {
                                    EntityDataValue::OptionalUnsignedInt(*v)
                                }
                                MetadataValue::OptionalEntity(v) => {
                                    EntityDataValue::OptionalLivingEntity(*v)
                                }
                                MetadataValue::OptionalBlockState(v) => {
                                    EntityDataValue::OptionalBlockState(v.map(|state| state as i32))
                                }
                                MetadataValue::VillagerData {
                                    kind,
                                    profession,
                                    level,
                                } => EntityDataValue::VillagerData {
                                    kind: *kind as i32,
                                    profession: *profession as i32,
                                    level: *level,
                                },
                            },
                        })
                        .collect(),
                },
            ),
            ClientEvent::EntityEquipment { entity, slots } => Self::queue(
                writer,
                &cb::SetEquipment {
                    entity_id: entity.0,
                    slots: slots
                        .iter()
                        .map(|(slot, item)| (equipment_slot(*slot), stack(item)))
                        .collect(),
                },
            ),
            ClientEvent::RemoveEntities(entities) => Self::queue(
                writer,
                &cb::RemoveEntities {
                    entity_ids: entities.iter().map(|e| e.0).collect(),
                },
            ),
            ClientEvent::InventoryContent {
                state_id,
                slots,
                carried,
            } => Self::queue(
                writer,
                &cb::ContainerSetContent {
                    container_id: 0,
                    state_id: *state_id,
                    items: slots.iter().map(stack).collect(),
                    carried: stack(carried),
                },
            ),
            ClientEvent::InventorySlot {
                state_id,
                slot,
                stack: item,
            } => Self::queue(
                writer,
                &cb::ContainerSetSlot {
                    container_id: 0,
                    state_id: *state_id,
                    slot: *slot,
                    item: stack(item),
                },
            ),
            ClientEvent::OpenScreen {
                window,
                menu,
                title,
            } => Self::queue(
                writer,
                &cb::OpenScreen {
                    container_id: *window,
                    menu: *menu,
                    title: text(title),
                },
            ),
            ClientEvent::MerchantOffers {
                window,
                offers,
                level,
                xp,
                show_progress,
                can_restock,
            } => Self::queue(
                writer,
                &cb::MerchantOffers {
                    container_id: *window,
                    offers: offers.iter().map(trade_offer).collect(),
                    level: *level,
                    xp: *xp,
                    show_progress: *show_progress,
                    can_restock: *can_restock,
                },
            ),
            ClientEvent::ContainerContent {
                window,
                state_id,
                slots,
                carried,
            } => Self::queue(
                writer,
                &cb::ContainerSetContent {
                    container_id: *window,
                    state_id: *state_id,
                    items: slots.iter().map(stack).collect(),
                    carried: stack(carried),
                },
            ),
            ClientEvent::ContainerSlot {
                window,
                state_id,
                slot,
                stack: item,
            } => Self::queue(
                writer,
                &cb::ContainerSetSlot {
                    container_id: *window,
                    state_id: *state_id,
                    slot: *slot,
                    item: stack(item),
                },
            ),
            ClientEvent::ContainerData { window, id, value } => Self::queue(
                writer,
                &cb::ContainerSetData {
                    container_id: *window,
                    id: *id,
                    value: *value,
                },
            ),
            ClientEvent::ContainerClose(window) => Self::queue(
                writer,
                &cb::ContainerClose {
                    container_id: *window,
                },
            ),
            ClientEvent::CursorItem(item) => {
                Self::queue(writer, &cb::SetCursorItem { item: stack(item) })
            }
            ClientEvent::PlayerInventorySlot { slot, stack: item } => Self::queue(
                writer,
                &cb::SetPlayerInventory {
                    slot: *slot,
                    item: stack(item),
                },
            ),
            ClientEvent::Health {
                health,
                food,
                saturation,
            } => Self::queue(
                writer,
                &cb::SetHealth {
                    health: *health,
                    food: *food,
                    saturation: *saturation,
                },
            ),
            ClientEvent::Experience {
                progress,
                level,
                total,
            } => Self::queue(
                writer,
                &cb::SetExperience {
                    progress: *progress,
                    level: *level,
                    total: *total,
                },
            ),
            ClientEvent::Commands(tree) => Self::queue(writer, &commands(tree)),
            ClientEvent::CommandSuggestions {
                id,
                start,
                length,
                matches,
            } => Self::queue(
                writer,
                &cb::CommandSuggestions {
                    id: *id,
                    start: *start,
                    length: *length,
                    suggestions: matches
                        .iter()
                        .map(|m| Suggestion {
                            text: m.clone(),
                            tooltip: None,
                        })
                        .collect(),
                },
            ),
            ClientEvent::ItemPickup {
                collected,
                collector,
                amount,
            } => Self::queue(
                writer,
                &cb::TakeItemEntity {
                    item_id: collected.0,
                    player_id: collector.0,
                    amount: *amount,
                },
            ),
            ClientEvent::OperatorLevel { entity, level } => Self::queue(
                writer,
                &cb::EntityEvent {
                    entity_id: entity.0,
                    event: cb::EntityEvent::OPERATOR_LEVEL_BASE + (*level).min(4) as i8,
                },
            ),
            ClientEvent::EntityStatus { entity, status } => Self::queue(
                writer,
                &cb::EntityEvent {
                    entity_id: entity.0,
                    event: match status {
                        EntityStatus::Death => cb::EntityEvent::DEATH,
                        EntityStatus::Poof => cb::EntityEvent::POOF,
                        EntityStatus::EquipmentBreak(slot) => equipment_break_event(*slot),
                        EntityStatus::Event(event) => *event,
                    },
                },
            ),
            ClientEvent::DamageEvent {
                entity,
                kind,
                cause,
                direct,
                position: at,
            } => match kind.registry_id() {
                Some(source_type) => Self::queue(
                    writer,
                    &cb::DamageEvent {
                        entity_id: entity.0,
                        source_type,
                        source_cause_id: cause.map(|e| e.0),
                        source_direct_id: direct.map(|e| e.0),
                        source_position: at.map(wire_vec),
                    },
                ),
                None => Ok(()),
            },
            ClientEvent::HurtAnimation { entity, yaw } => Self::queue(
                writer,
                &cb::HurtAnimation {
                    entity_id: entity.0,
                    yaw: *yaw,
                },
            ),
            ClientEvent::CombatKill { player, message } => Self::queue(
                writer,
                &cb::PlayerCombatKill {
                    player_id: player.0,
                    message: text(message),
                },
            ),
            ClientEvent::EffectApplied {
                entity,
                effect,
                blend,
            } => Self::queue(
                writer,
                &cb::UpdateMobEffect {
                    entity_id: entity.0,
                    effect: i32::from(effect.effect.0),
                    amplifier: i32::from(effect.amplifier),
                    duration: effect.duration.wire_ticks(),
                    flags: effect_flags(effect, *blend),
                },
            ),
            ClientEvent::EffectRemoved { entity, effect } => Self::queue(
                writer,
                &cb::RemoveMobEffect {
                    entity_id: entity.0,
                    effect: i32::from(effect.0),
                },
            ),
            ClientEvent::Attributes { entity, attributes } => Self::queue(
                writer,
                &cb::UpdateAttributes {
                    entity_id: entity.0,
                    attributes: attributes.iter().filter_map(attribute_snapshot).collect(),
                },
            ),
            ClientEvent::Sound {
                sound,
                source,
                position: at,
                volume,
                pitch,
                seed,
            } => match rspigot_data::registries::static_index("minecraft:sound_event", sound)
                .and_then(|id| i32::try_from(id).ok())
            {
                Some(id) => Self::queue(
                    writer,
                    &cb::Sound::at(
                        Holder::Reference(id),
                        sound_source(*source),
                        (at.x, at.y, at.z),
                        *volume,
                        *pitch,
                        *seed,
                    ),
                ),
                None => Ok(()),
            },
            ClientEvent::Explode {
                center,
                radius,
                block_count,
                knockback,
                large,
                play_sound,
                gust,
            } => {
                let particle = match (gust, *large) {
                    (Some(_), true) => "minecraft:gust_emitter_large",
                    (Some(_), false) => "minecraft:gust_emitter_small",
                    (None, true) => "minecraft:explosion_emitter",
                    (None, false) => "minecraft:explosion",
                };
                let (Some(particle), Some(sound)) = (
                    particle_options(particle, &ParticleData::None),
                    rspigot_data::registries::static_index(
                        "minecraft:sound_event",
                        gust.unwrap_or("minecraft:entity.generic.explode"),
                    )
                    .and_then(|id| i32::try_from(id).ok()),
                ) else {
                    return Ok(());
                };
                let block_particles = [("minecraft:poof", 0.5), ("minecraft:smoke", 1.0)]
                    .into_iter()
                    .filter(|_| gust.is_none())
                    .filter_map(|(name, scaling)| {
                        Some(cb::ExplosionParticle {
                            particle: particle_options(name, &ParticleData::None)?,
                            scaling,
                            speed: 1.0,
                            weight: 1,
                        })
                    })
                    .collect();
                Self::queue(
                    writer,
                    &cb::Explode {
                        x: center.x,
                        y: center.y,
                        z: center.z,
                        radius: *radius,
                        block_count: *block_count,
                        knockback: knockback.map(|v| (v.x, v.y, v.z)),
                        particle,
                        sound: Holder::Reference(sound),
                        block_particles,
                        play_sound: *play_sound,
                    },
                )
            }
            ClientEvent::LevelEvent { event, pos, data } => Self::queue(
                writer,
                &cb::LevelEvent {
                    kind: *event,
                    pos: position(*pos),
                    data: *data,
                    global: false,
                },
            ),
            ClientEvent::Particle(particle) => {
                let Some(options) = particle_options(particle.kind, &particle.data) else {
                    return Ok(());
                };
                Self::queue(
                    writer,
                    &cb::LevelParticles {
                        particle: options,
                        override_limiter: false,
                        always_show: false,
                        x: particle.position.x,
                        y: particle.position.y,
                        z: particle.position.z,
                        x_dist: particle.spread.x as f32,
                        y_dist: particle.spread.y as f32,
                        z_dist: particle.spread.z as f32,
                        x_max_speed: particle.speed as f32,
                        y_max_speed: particle.speed as f32,
                        z_max_speed: particle.speed as f32,
                        count: particle.count,
                        randomization: cb::ParticleRandomization::Default,
                    },
                )
            }
            ClientEvent::Disconnect(_) => Ok(()),
        }
    }

    fn entity_move<W: PacketSink>(
        writer: &mut W,
        entity: i32,
        movement: &Movement,
    ) -> Result<(), ConnectionError> {
        match movement {
            Movement::Position { delta, on_ground } => Self::queue(
                writer,
                &cb::MoveEntityPos {
                    entity_id: entity,
                    on_ground: *on_ground,
                    delta: wire_delta(delta),
                },
            ),
            Movement::PositionRotation {
                delta,
                yaw,
                pitch,
                on_ground,
            } => Self::queue(
                writer,
                &cb::MoveEntityPosRot {
                    entity_id: entity,
                    on_ground: *on_ground,
                    delta: wire_delta(delta),
                    y_rot: Angle(*yaw),
                    x_rot: Angle(*pitch),
                },
            ),
            Movement::Rotation {
                yaw,
                pitch,
                on_ground,
            } => Self::queue(
                writer,
                &cb::MoveEntityRot {
                    entity_id: entity,
                    on_ground: *on_ground,
                    y_rot: Angle(*yaw),
                    x_rot: Angle(*pitch),
                },
            ),
            Movement::Sync {
                path,
                yaw,
                pitch,
                on_ground,
            } => Self::queue(
                writer,
                &cb::EntityPositionSync {
                    entity_id: entity,
                    position: match path {
                        CorePath::Linear(end) => PositionPath::Linear(wire_vec(*end)),
                        CorePath::Stepped(steps) => PositionPath::Stepped(
                            steps
                                .iter()
                                .map(|(position, ticks)| PositionStep {
                                    position: wire_vec(*position),
                                    tick_offset: *ticks,
                                })
                                .collect(),
                        ),
                    },
                    y_rot: *yaw,
                    x_rot: *pitch,
                    on_ground: *on_ground,
                },
            ),
        }
    }

    fn entity_moved<W: PacketSink>(
        writer: &mut W,
        entity: i32,
        from: Vec3,
        to: Vec3,
        rotation: Option<Rotation>,
        on_ground: bool,
    ) -> Result<(), ConnectionError> {
        let moved = fixed(from.x) != fixed(to.x)
            || fixed(from.y) != fixed(to.y)
            || fixed(from.z) != fixed(to.z);
        match (moved, delta(from, to), rotation) {
            (false, _, Some(rotation)) => Self::queue(
                writer,
                &cb::MoveEntityRot {
                    entity_id: entity,
                    on_ground,
                    y_rot: Angle::from_degrees(rotation.yaw),
                    x_rot: Angle::from_degrees(rotation.pitch),
                },
            ),
            (false, _, None) => Ok(()),
            (true, Some(step), None) => Self::queue(
                writer,
                &cb::MoveEntityPos {
                    entity_id: entity,
                    on_ground,
                    delta: step,
                },
            ),
            (true, Some(step), Some(rotation)) => Self::queue(
                writer,
                &cb::MoveEntityPosRot {
                    entity_id: entity,
                    on_ground,
                    delta: step,
                    y_rot: Angle::from_degrees(rotation.yaw),
                    x_rot: Angle::from_degrees(rotation.pitch),
                },
            ),
            (true, None, rotation) => {
                let rotation = rotation.unwrap_or_default();
                Self::queue(
                    writer,
                    &cb::EntityPositionSync {
                        entity_id: entity,
                        position: PositionPath::Linear(wire_vec(to)),
                        y_rot: rotation.yaw,
                        x_rot: rotation.pitch,
                        on_ground,
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::events::{DeltaStep as CoreStep, MetadataEntry};
    use rspigot_domain::text::Component;
    use rspigot_domain::{BlockPos, EntityId};
    use rspigot_protocol::ids::play::clientbound as id;

    use super::*;

    #[derive(Default)]
    struct Captured(Vec<(i32, Vec<u8>)>);

    impl PacketSink for Captured {
        fn queue<P: Encode + ?Sized>(
            &mut self,
            id: i32,
            packet: &P,
        ) -> Result<(), ConnectionError> {
            let mut body = Vec::new();
            packet.encode(&mut body);
            self.0.push((id, body));
            Ok(())
        }

        fn queue_prebuilt(&mut self, _: &[u8]) {}
    }

    fn encode(event: ClientEvent) -> Vec<(i32, Vec<u8>)> {
        let mut sink = Captured::default();
        Encoder::new(false)
            .encode(&event, &mut sink, &ChunkPacketCache::new(1024), None, 0)
            .unwrap();
        sink.0
    }

    #[test]
    fn bundle_delimiter_is_an_empty_packet() {
        assert_eq!(
            encode(ClientEvent::BundleDelimiter),
            vec![(id::BUNDLE_DELIMITER, Vec::new())]
        );
    }

    #[test]
    fn linear_position_move_golden_bytes() {
        let event = ClientEvent::EntityMove {
            entity: EntityId(5),
            movement: Movement::Position {
                delta: MoveDelta::Linear {
                    xa: 0,
                    ya: -2048,
                    za: 1,
                },
                on_ground: true,
            },
        };
        assert_eq!(
            encode(event),
            vec![(id::MOVE_ENTITY_POS, vec![5, 1, 0, 0, 0xF8, 0x00, 0, 1])]
        );
    }

    #[test]
    fn stepped_position_rotation_move_golden_bytes() {
        let event = ClientEvent::EntityMove {
            entity: EntityId(1),
            movement: Movement::PositionRotation {
                delta: MoveDelta::Stepped(vec![CoreStep {
                    ticks: 3,
                    xa: 0,
                    ya: -2048,
                    za: 0,
                }]),
                yaw: 64,
                pitch: 192,
                on_ground: false,
            },
        };
        assert_eq!(
            encode(event),
            vec![(
                id::MOVE_ENTITY_POS_ROT,
                vec![1, 2, 3, 0, 0, 0xF8, 0x00, 0, 0, 64, 192]
            )]
        );
    }

    #[test]
    fn rotation_move_puts_on_ground_before_the_angles() {
        let event = ClientEvent::EntityMove {
            entity: EntityId(9),
            movement: Movement::Rotation {
                yaw: 1,
                pitch: 2,
                on_ground: true,
            },
        };
        assert_eq!(encode(event), vec![(id::MOVE_ENTITY_ROT, vec![9, 1, 1, 2])]);
    }

    #[test]
    fn position_sync_golden_bytes() {
        let event = ClientEvent::EntityMove {
            entity: EntityId(2),
            movement: Movement::Sync {
                path: CorePath::Linear(Vec3::new(1.0, 64.0, -2.5)),
                yaw: 90.0,
                pitch: 0.0,
                on_ground: true,
            },
        };
        let mut expected = vec![2, 0];
        expected.extend_from_slice(&1.0f64.to_be_bytes());
        expected.extend_from_slice(&64.0f64.to_be_bytes());
        expected.extend_from_slice(&(-2.5f64).to_be_bytes());
        expected.extend_from_slice(&90.0f32.to_be_bytes());
        expected.extend_from_slice(&0.0f32.to_be_bytes());
        expected.push(1);
        assert_eq!(encode(event), vec![(id::ENTITY_POSITION_SYNC, expected)]);
    }

    #[test]
    fn falling_block_start_pos_metadata_golden_bytes() {
        let event = ClientEvent::EntityMetadata {
            entity: EntityId(3),
            entries: vec![MetadataEntry {
                index: 8,
                value: MetadataValue::BlockPos(BlockPos::new(1, 64, -1)),
            }],
        };
        let packed: i64 = (1i64 << 38) | (0x3FF_FFFF << 12) | 64;
        let mut expected = vec![3, 8, 10];
        expected.extend_from_slice(&packed.to_be_bytes());
        expected.push(0xFF);
        assert_eq!(encode(event), vec![(id::SET_ENTITY_DATA, expected)]);
    }

    fn stone(count: u8) -> rspigot_domain::item::Slot {
        Some(ItemStack::new(
            rspigot_domain::ItemId(1),
            std::num::NonZeroU8::new(count).unwrap(),
        ))
    }

    #[test]
    fn cursor_item_golden_bytes() {
        assert_eq!(
            encode(ClientEvent::CursorItem(stone(3))),
            vec![(id::SET_CURSOR_ITEM, vec![3, 1, 0, 0])]
        );
        assert_eq!(
            encode(ClientEvent::CursorItem(None)),
            vec![(id::SET_CURSOR_ITEM, vec![0])]
        );
    }

    #[test]
    fn player_inventory_slot_golden_bytes() {
        assert_eq!(
            encode(ClientEvent::PlayerInventorySlot {
                slot: 40,
                stack: stone(2),
            }),
            vec![(id::SET_PLAYER_INVENTORY, vec![40, 2, 1, 0, 0])]
        );
    }

    #[test]
    fn inventory_content_carries_the_cursor_golden_bytes() {
        assert_eq!(
            encode(ClientEvent::InventoryContent {
                state_id: 7,
                slots: vec![None, stone(64)],
                carried: stone(5),
            }),
            vec![(
                id::CONTAINER_SET_CONTENT,
                vec![0, 7, 2, 0, 64, 1, 0, 0, 5, 1, 0, 0]
            )]
        );
        assert_eq!(
            encode(ClientEvent::InventorySlot {
                state_id: 8,
                slot: 0,
                stack: None,
            }),
            vec![(id::CONTAINER_SET_SLOT, vec![0, 8, 0, 0, 0])]
        );
    }

    #[test]
    fn block_events_carry_the_action_the_data_and_the_block() {
        let packets = encode(ClientEvent::BlockEvent {
            pos: BlockPos::new(1, 64, 2),
            action: 1,
            data: 2,
            block: 130,
        });
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, id::BLOCK_EVENT);
        assert_eq!(&packets[0].1[8..], &[1, 2, 130, 1]);
    }

    #[test]
    fn merchant_offers_carry_the_base_costs_the_stock_and_the_price_state() {
        use rspigot_domain::ItemId;
        use rspigot_domain::entity::trading::{ItemCost, MerchantOffer};
        let mut water = ItemCost::new(ItemId(5), 1);
        water.components.insert(
            "minecraft:potion_contents",
            rspigot_nbt::Compound::new().with("potion", "minecraft:water"),
        );
        let mut offer = MerchantOffer::new(
            water,
            Some(ItemCost::new(ItemId(6), 2)),
            ItemStack::new(ItemId(7), std::num::NonZeroU8::new(3).unwrap()),
            12,
            5,
            0.5,
        );
        offer.uses = 12;
        offer.special_price_diff = -2;
        offer.demand = 7;
        let packets = encode(ClientEvent::MerchantOffers {
            window: 4,
            offers: vec![offer],
            level: 3,
            xp: 99,
            show_progress: true,
            can_restock: false,
        });
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, id::MERCHANT_OFFERS);
        let body = &packets[0].1;
        let potion_contents = rspigot_data::registries::static_index(
            "minecraft:data_component_type",
            "minecraft:potion_contents",
        )
        .unwrap() as u8;
        assert_eq!(&body[..6], &[4, 1, 5, 1, 1, potion_contents]);
        let tail = &body[body.len() - 31..];
        assert_eq!(
            tail,
            &[
                2, 0, 1, 0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0, 5, 0xFF, 0xFF, 0xFF, 0xFE, 0x3F, 0, 0,
                0, 0, 0, 0, 7, 3, 99, 1, 0
            ]
        );
    }

    #[test]
    fn container_data_carries_the_window_the_property_and_the_value() {
        assert_eq!(
            encode(ClientEvent::ContainerData {
                window: 2,
                id: 3,
                value: 200
            }),
            vec![(id::CONTAINER_SET_DATA, vec![2, 0, 3, 0, 200])]
        );
    }

    #[test]
    fn container_windows_carry_their_id() {
        assert_eq!(
            encode(ClientEvent::ContainerContent {
                window: 3,
                state_id: 7,
                slots: vec![None, stone(64)],
                carried: None,
            }),
            vec![(id::CONTAINER_SET_CONTENT, vec![3, 7, 2, 0, 64, 1, 0, 0, 0])]
        );
        assert_eq!(
            encode(ClientEvent::ContainerSlot {
                window: 3,
                state_id: 8,
                slot: 26,
                stack: None,
            }),
            vec![(id::CONTAINER_SET_SLOT, vec![3, 8, 0, 26, 0])]
        );
        assert_eq!(
            encode(ClientEvent::ContainerClose(3)),
            vec![(id::CONTAINER_CLOSE, vec![3])]
        );
    }

    #[test]
    fn opening_a_screen_starts_with_the_window_and_the_menu_type() {
        let packets = encode(ClientEvent::OpenScreen {
            window: 5,
            menu: 2,
            title: rspigot_domain::text::Component::translatable("container.chest", Vec::new()),
        });
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, id::OPEN_SCREEN);
        assert_eq!(&packets[0].1[..2], &[5, 2]);
    }

    #[test]
    fn positional_sound_golden_bytes() {
        let event = ClientEvent::Sound {
            sound: "minecraft:entity.cow.hurt",
            source: CoreSoundSource::Neutral,
            position: Vec3::new(0.5, 64.0, -2.56),
            volume: 0.4,
            pitch: 1.0,
            seed: 0x0102_0304_0506_0708,
        };
        let mut expected = vec![0xC4, 0x03, 6];
        expected.extend_from_slice(&4i32.to_be_bytes());
        expected.extend_from_slice(&512i32.to_be_bytes());
        expected.extend_from_slice(&(-20i32).to_be_bytes());
        expected.extend_from_slice(&0.4f32.to_be_bytes());
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.extend_from_slice(&0x0102_0304_0506_0708i64.to_be_bytes());
        assert_eq!(encode(event), vec![(id::SOUND, expected)]);
        let unknown = ClientEvent::Sound {
            sound: "minecraft:nope",
            source: CoreSoundSource::Neutral,
            position: Vec3::ZERO,
            volume: 1.0,
            pitch: 1.0,
            seed: 0,
        };
        assert!(encode(unknown).is_empty());
    }

    #[test]
    fn entity_events_golden_bytes() {
        let death = ClientEvent::EntityStatus {
            entity: EntityId(0x0102),
            status: EntityStatus::Death,
        };
        assert_eq!(encode(death), vec![(id::ENTITY_EVENT, vec![0, 0, 1, 2, 3])]);
        let poof = ClientEvent::EntityStatus {
            entity: EntityId(7),
            status: EntityStatus::Poof,
        };
        assert_eq!(encode(poof), vec![(id::ENTITY_EVENT, vec![0, 0, 0, 7, 60])]);
        let broke = ClientEvent::EntityStatus {
            entity: EntityId(7),
            status: EntityStatus::EquipmentBreak(CoreSlot::MainHand),
        };
        assert_eq!(
            encode(broke),
            vec![(id::ENTITY_EVENT, vec![0, 0, 0, 7, 47])]
        );
        let body = ClientEvent::EntityStatus {
            entity: EntityId(7),
            status: EntityStatus::EquipmentBreak(CoreSlot::Body),
        };
        assert_eq!(encode(body), vec![(id::ENTITY_EVENT, vec![0, 0, 0, 7, 65])]);
    }

    fn named(name: &str) -> rspigot_domain::ItemId {
        let id = rspigot_data::registries::static_index("minecraft:item", name).unwrap();
        rspigot_domain::ItemId(u16::try_from(id).unwrap())
    }

    #[test]
    fn damaged_sword_slot_carries_the_damage_component() {
        let mut sword = ItemStack::new(named("minecraft:diamond_sword"), std::num::NonZeroU8::MIN);
        sword.components.insert("minecraft:damage", 12);
        let event = ClientEvent::InventorySlot {
            state_id: 3,
            slot: 36,
            stack: Some(sword),
        };
        assert_eq!(
            encode(event),
            vec![(
                id::CONTAINER_SET_SLOT,
                vec![0, 3, 0, 36, 1, 0x9A, 0x08, 1, 0, 3, 12]
            )]
        );
    }

    #[test]
    fn potion_and_ominous_components_golden_bytes() {
        let mut arrow = ItemStack::new(named("minecraft:tipped_arrow"), std::num::NonZeroU8::MIN);
        arrow.components.insert(
            "minecraft:potion_contents",
            rspigot_nbt::Compound::new().with("potion", "minecraft:poison"),
        );
        let wire = stack(&Some(arrow));
        let mut body = Vec::new();
        wire.encode(&mut body);
        assert_eq!(body, vec![1, 0xA4, 0x0B, 1, 0, 53, 1, 28, 0, 0, 0]);
        let mut bottle =
            ItemStack::new(named("minecraft:ominous_bottle"), std::num::NonZeroU8::MIN);
        bottle
            .components
            .insert("minecraft:ominous_bottle_amplifier", 4);
        let mut body = Vec::new();
        stack(&Some(bottle)).encode(&mut body);
        assert_eq!(body, vec![1, 0xF9, 0x0C, 1, 0, 65, 4]);
        let mut stew = ItemStack::new(named("minecraft:suspicious_stew"), std::num::NonZeroU8::MIN);
        stew.components.insert(
            "minecraft:suspicious_stew_effects",
            rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(
                rspigot_nbt::Compound::new()
                    .with("id", "minecraft:fire_resistance")
                    .with("duration", 60),
            )]),
        );
        let mut body = Vec::new();
        stack(&Some(stew)).encode(&mut body);
        let effect = rspigot_data::registries::static_index(
            "minecraft:mob_effect",
            "minecraft:fire_resistance",
        )
        .unwrap();
        let component = component_type("minecraft:suspicious_stew_effects").unwrap();
        assert_eq!(
            body[body.len() - 6..],
            [
                1,
                0,
                u8::try_from(component).unwrap(),
                1,
                u8::try_from(effect).unwrap(),
                60
            ]
        );
        let mut unknown = ItemStack::new(named("minecraft:stick"), std::num::NonZeroU8::MIN);
        unknown.components.insert("minecraft:custom_data", 1);
        let mut body = Vec::new();
        stack(&Some(unknown)).encode(&mut body);
        assert_eq!(body[body.len() - 2..], [0, 0]);
    }

    fn component_bytes(name: &str, value: impl Into<rspigot_nbt::Tag>) -> Vec<u8> {
        component_value(name, &value.into()).expect(name).data
    }

    fn explosion_tag() -> rspigot_nbt::Compound {
        rspigot_nbt::Compound::new()
            .with("shape", "star")
            .with(
                "colors",
                rspigot_nbt::Tag::List(vec![
                    rspigot_nbt::Tag::Int(0xB3312C),
                    rspigot_nbt::Tag::Int(0x253192),
                ]),
            )
            .with("has_trail", true)
    }

    fn explosion_bytes() -> Vec<u8> {
        let mut bytes = vec![2, 2];
        bytes.extend_from_slice(&0xB3312Ci32.to_be_bytes());
        bytes.extend_from_slice(&0x253192i32.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0]);
        bytes
    }

    #[test]
    fn dye_and_firework_components_golden_bytes() {
        assert_eq!(
            component_bytes("minecraft:dyed_color", 0xB02E26),
            0xB02E26i32.to_be_bytes()
        );
        assert_eq!(component_bytes("minecraft:base_color", "red"), vec![14]);
        assert_eq!(
            component_bytes("minecraft:firework_explosion", explosion_tag()),
            explosion_bytes()
        );
        let fireworks = rspigot_nbt::Compound::new()
            .with("flight_duration", rspigot_nbt::Tag::Byte(2))
            .with(
                "explosions",
                rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(explosion_tag())]),
            );
        let mut expected = vec![2, 1];
        expected.extend(explosion_bytes());
        assert_eq!(component_bytes("minecraft:fireworks", fireworks), expected);
        let bare = rspigot_nbt::Compound::new().with("flight_duration", rspigot_nbt::Tag::Byte(3));
        assert_eq!(component_bytes("minecraft:fireworks", bare), vec![3, 0]);
    }

    #[test]
    fn name_lore_and_enchantments_golden_bytes() {
        assert_eq!(
            component_bytes("minecraft:custom_name", "A"),
            vec![8, 0, 1, b'A']
        );
        let lore = rspigot_nbt::Tag::List(vec![
            rspigot_nbt::Tag::String("A".to_owned()),
            rspigot_nbt::Tag::String("B".to_owned()),
        ]);
        assert_eq!(
            component_bytes("minecraft:lore", lore),
            vec![2, 8, 0, 1, b'A', 8, 0, 1, b'B']
        );
        let id =
            rspigot_data::registries::synced_index("minecraft:enchantment", "minecraft:sharpness")
                .unwrap();
        let expected = vec![1, u8::try_from(id).unwrap(), 5];
        assert_eq!(
            component_bytes(
                "minecraft:enchantments",
                rspigot_nbt::Compound::new().with("minecraft:sharpness", 5)
            ),
            expected
        );
        let levels = rspigot_nbt::Compound::new().with("minecraft:sharpness", 5);
        assert_eq!(
            component_bytes(
                "minecraft:enchantments",
                rspigot_nbt::Compound::new().with("levels", levels)
            ),
            expected
        );
        let styled = rspigot_nbt::Compound::new()
            .with("text", "A")
            .with("color", "gold");
        let mut gold = Vec::new();
        rspigot_nbt::write_network(&mut gold, &rspigot_nbt::Tag::Compound(styled.clone()));
        assert_eq!(component_bytes("minecraft:custom_name", styled), gold);
        let unknown = rspigot_nbt::Compound::new().with("minecraft:nope", 1);
        assert!(
            component_value(
                "minecraft:enchantments",
                &rspigot_nbt::Tag::Compound(unknown)
            )
            .is_none()
        );
        let too_long = rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::String("A".to_owned()); 257]);
        assert!(component_value("minecraft:lore", &too_long).is_none());
    }

    #[test]
    fn a_reset_name_reaches_the_client_without_italic() {
        let name = rspigot_domain::entity::save::component_to_tag(&Component::from_section_codes(
            "§rPlain",
        ));
        let bytes = component_bytes("minecraft:custom_name", name);
        let Some(rspigot_nbt::Tag::Compound(sent)) =
            rspigot_nbt::read_network(&mut bytes.as_slice()).unwrap()
        else {
            panic!("the name is sent as a compound");
        };
        assert_eq!(sent.get_str("text"), Some("Plain"));
        for flag in [
            "bold",
            "italic",
            "underlined",
            "strikethrough",
            "obfuscated",
        ] {
            assert_eq!(sent.get(flag), Some(&rspigot_nbt::Tag::Byte(0)), "{flag}");
        }
        assert!(sent.get("color").is_none());
    }

    #[test]
    fn the_trim_component_names_its_material_then_its_pattern_by_registry_id() {
        let trim = rspigot_nbt::Compound::new()
            .with("material", "minecraft:copper")
            .with("pattern", "minecraft:bolt");
        let material =
            rspigot_data::registries::synced_index("minecraft:trim_material", "minecraft:copper")
                .unwrap();
        let pattern =
            rspigot_data::registries::synced_index("minecraft:trim_pattern", "minecraft:bolt")
                .unwrap();
        assert_eq!(
            component_bytes("minecraft:trim", trim),
            vec![
                u8::try_from(material + 1).unwrap(),
                u8::try_from(pattern + 1).unwrap()
            ]
        );
        let unknown = rspigot_nbt::Compound::new()
            .with("material", "minecraft:nope")
            .with("pattern", "minecraft:bolt");
        assert!(component_value("minecraft:trim", &rspigot_nbt::Tag::Compound(unknown)).is_none());
    }

    #[test]
    fn banner_and_pot_components_golden_bytes() {
        let stripe = rspigot_data::registries::synced_index(
            "minecraft:banner_pattern",
            "minecraft:stripe_bottom",
        )
        .unwrap();
        let layer = rspigot_nbt::Compound::new()
            .with("pattern", "minecraft:stripe_bottom")
            .with("color", "red");
        assert_eq!(
            component_bytes(
                "minecraft:banner_patterns",
                rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(layer)])
            ),
            vec![1, u8::try_from(stripe + 1).unwrap(), 14]
        );
        let brick =
            rspigot_data::registries::static_index("minecraft:item", "minecraft:brick").unwrap();
        let sides = rspigot_nbt::Compound::new().with(
            "back",
            rspigot_nbt::Compound::new().with("id", "minecraft:brick"),
        );
        let mut expected = vec![1];
        write_var(i32::try_from(brick).unwrap(), &mut expected);
        expected.extend([1, 0, 0, 0, 0, 0]);
        assert_eq!(
            component_bytes("minecraft:pot_decorations", sides),
            expected
        );
    }

    #[test]
    fn experience_and_pickup_golden_bytes() {
        let xp = ClientEvent::Experience {
            progress: 0.5,
            level: 3,
            total: 300,
        };
        let mut expected = 0.5f32.to_be_bytes().to_vec();
        expected.extend_from_slice(&[3, 0xAC, 0x02]);
        assert_eq!(encode(xp), vec![(id::SET_EXPERIENCE, expected)]);
        let pickup = ClientEvent::ItemPickup {
            collected: EntityId(300),
            collector: EntityId(2),
            amount: 1,
        };
        assert_eq!(
            encode(pickup),
            vec![(id::TAKE_ITEM_ENTITY, vec![0xAC, 0x02, 2, 1])]
        );
    }

    #[test]
    fn block_entity_update_golden_bytes() {
        let tag = rspigot_nbt::Compound::new().with("Items", rspigot_nbt::Tag::List(Vec::new()));
        let event = ClientEvent::BlockEntityUpdate {
            pos: BlockPos::new(1, 64, 2),
            kind: "campfire",
            data: tag.clone(),
        };
        let campfire = rspigot_data::registries::static_index(
            "minecraft:block_entity_type",
            "minecraft:campfire",
        )
        .unwrap();
        let mut expected = Vec::new();
        position(BlockPos::new(1, 64, 2)).encode(&mut expected);
        write_var(i32::try_from(campfire).unwrap(), &mut expected);
        rspigot_nbt::write_network(&mut expected, &rspigot_nbt::Tag::Compound(tag));
        assert_eq!(encode(event), vec![(id::BLOCK_ENTITY_DATA, expected)]);
        let unknown = ClientEvent::BlockEntityUpdate {
            pos: BlockPos::new(0, 0, 0),
            kind: "nope",
            data: rspigot_nbt::Compound::new(),
        };
        assert_eq!(encode(unknown), Vec::new());
    }

    #[test]
    fn the_recipe_sync_lists_the_sets_and_every_stonecutter_recipe() {
        let packet = update_recipes();
        assert_eq!(packet.property_sets.len(), 9);
        assert_eq!(packet.stonecutter.len(), 351);
        let first = &packet.stonecutter[0];
        let stone = i32::from(named("minecraft:stone").0);
        let chiseled = packet
            .stonecutter
            .iter()
            .find(|entry| {
                entry.display
                    == SlotDisplay::ItemStack(ItemStackTemplate {
                        item: i32::from(named("minecraft:chiseled_stone_bricks").0),
                        count: 1,
                        components: DataComponentPatch::EMPTY,
                    })
                    && entry.input == HolderSet::Ids(vec![stone])
            })
            .expect("chiseled stone bricks from stone");
        let mut bytes = Vec::new();
        chiseled.encode(&mut bytes);
        let mut expected = vec![2];
        write_var(stone, &mut expected);
        expected.push(5);
        write_var(
            i32::from(named("minecraft:chiseled_stone_bricks").0),
            &mut expected,
        );
        expected.extend([1, 0, 0]);
        assert_eq!(bytes, expected);
        assert!(matches!(&first.input, HolderSet::Ids(ids) if ids.len() == 1));
        let event = encode(ClientEvent::RecipeSync);
        assert_eq!(event.len(), 1);
        assert_eq!(event[0].0, id::UPDATE_RECIPES);
        assert_eq!(event[0].1[0], 9);
    }

    #[test]
    fn orb_spawn_golden_bytes() {
        let event = ClientEvent::SpawnEntity {
            entity: EntityId(5),
            uuid: uuid::Uuid::from_u128(1),
            kind: EntityKind::Other(50),
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Rotation::new(0.0, 0.0),
            head_yaw: 0.0,
            data: 0,
            velocity: Vec3::ZERO,
        };
        let mut expected = vec![5];
        expected.extend_from_slice(&0u64.to_be_bytes());
        expected.extend_from_slice(&1u64.to_be_bytes());
        expected.push(50);
        expected.extend_from_slice(&1.0f64.to_be_bytes());
        expected.extend_from_slice(&2.0f64.to_be_bytes());
        expected.extend_from_slice(&3.0f64.to_be_bytes());
        expected.extend_from_slice(&[0, 0, 0, 0, 0]);
        assert_eq!(encode(event), vec![(id::ADD_ENTITY, expected)]);
        let value = ClientEvent::EntityMetadata {
            entity: EntityId(5),
            entries: vec![MetadataEntry {
                index: 8,
                value: MetadataValue::VarInt(17),
            }],
        };
        assert_eq!(
            encode(value),
            vec![(id::SET_ENTITY_DATA, vec![5, 8, 1, 17, 0xFF])]
        );
    }

    #[test]
    fn farm_variant_wool_and_baby_metadata_golden_bytes() {
        let event = ClientEvent::EntityMetadata {
            entity: EntityId(9),
            entries: vec![
                MetadataEntry {
                    index: 16,
                    value: MetadataValue::Boolean(true),
                },
                MetadataEntry {
                    index: 18,
                    value: MetadataValue::Variant(VariantKind::Mob(VariantFamily::Cow), 2),
                },
                MetadataEntry {
                    index: 19,
                    value: MetadataValue::Variant(VariantKind::Sound(VariantFamily::Cow), 1),
                },
                MetadataEntry {
                    index: 20,
                    value: MetadataValue::Variant(VariantKind::Sound(VariantFamily::Pig), 2),
                },
                MetadataEntry {
                    index: 19,
                    value: MetadataValue::Variant(VariantKind::Mob(VariantFamily::Chicken), 0),
                },
                MetadataEntry {
                    index: 18,
                    value: MetadataValue::Byte(0x1E),
                },
            ],
        };
        assert_eq!(
            encode(event),
            vec![(
                id::SET_ENTITY_DATA,
                vec![
                    9, 16, 8, 1, 18, 23, 2, 19, 24, 1, 20, 29, 2, 19, 30, 0, 18, 0, 0x1E, 0xFF
                ]
            )]
        );
    }

    #[test]
    fn wolf_and_cat_variant_metadata_golden_bytes() {
        let event = ClientEvent::EntityMetadata {
            entity: EntityId(9),
            entries: vec![
                MetadataEntry {
                    index: 23,
                    value: MetadataValue::Variant(VariantKind::Mob(VariantFamily::Wolf), 3),
                },
                MetadataEntry {
                    index: 24,
                    value: MetadataValue::Variant(VariantKind::Sound(VariantFamily::Wolf), 2),
                },
                MetadataEntry {
                    index: 20,
                    value: MetadataValue::Variant(VariantKind::Mob(VariantFamily::Cat), 1),
                },
                MetadataEntry {
                    index: 24,
                    value: MetadataValue::Variant(VariantKind::Sound(VariantFamily::Cat), 1),
                },
            ],
        };
        assert_eq!(
            encode(event),
            vec![(
                id::SET_ENTITY_DATA,
                vec![9, 23, 25, 3, 24, 26, 2, 20, 21, 1, 24, 22, 1, 0xFF]
            )]
        );
    }

    #[test]
    fn zombie_villager_data_metadata_golden_bytes() {
        let event = ClientEvent::EntityMetadata {
            entity: EntityId(9),
            entries: vec![
                MetadataEntry {
                    index: 19,
                    value: MetadataValue::Boolean(true),
                },
                MetadataEntry {
                    index: 20,
                    value: MetadataValue::VillagerData {
                        kind: 5,
                        profession: 9,
                        level: 3,
                    },
                },
            ],
        };
        assert_eq!(
            encode(event),
            vec![(
                id::SET_ENTITY_DATA,
                vec![9, 19, 8, 1, 20, 18, 5, 9, 3, 0xFF]
            )]
        );
    }

    #[test]
    fn shulker_attach_face_metadata_golden_bytes() {
        let faces = [(0u8, 0u8), (1, 1), (2, 2), (3, 3), (4, 4), (5, 5)];
        for (index, wire) in faces {
            let event = ClientEvent::EntityMetadata {
                entity: EntityId(9),
                entries: vec![MetadataEntry {
                    index: 16,
                    value: MetadataValue::Direction(index),
                }],
            };
            assert_eq!(
                encode(event),
                vec![(id::SET_ENTITY_DATA, vec![9, 16, 12, wire, 0xFF])]
            );
        }
    }

    #[test]
    fn love_and_eat_grass_events_golden_bytes() {
        let love = ClientEvent::EntityStatus {
            entity: EntityId(7),
            status: EntityStatus::Event(18),
        };
        assert_eq!(encode(love), vec![(id::ENTITY_EVENT, vec![0, 0, 0, 7, 18])]);
        let grass = ClientEvent::EntityStatus {
            entity: EntityId(7),
            status: EntityStatus::Event(10),
        };
        assert_eq!(
            encode(grass),
            vec![(id::ENTITY_EVENT, vec![0, 0, 0, 7, 10])]
        );
    }

    #[test]
    fn level_event_golden_bytes() {
        let event = ClientEvent::LevelEvent {
            event: 2001,
            pos: BlockPos::new(1, 64, -1),
            data: 9,
        };
        let packed: i64 = (1i64 << 38) | (0x3FF_FFFF << 12) | 64;
        let mut expected = 2001i32.to_be_bytes().to_vec();
        expected.extend_from_slice(&packed.to_be_bytes());
        expected.extend_from_slice(&9i32.to_be_bytes());
        expected.push(0);
        assert_eq!(encode(event), vec![(id::LEVEL_EVENT, expected)]);
    }

    fn registry_id(registry: &str, name: &str) -> u8 {
        u8::try_from(rspigot_data::registries::static_index(registry, name).unwrap()).unwrap()
    }

    #[test]
    fn item_particle_golden_bytes() {
        let wheat = named("minecraft:wheat");
        let event = ClientEvent::Particle(rspigot_domain::particle::Particle {
            kind: "minecraft:item",
            data: ParticleData::Item(wheat),
            position: Vec3::new(0.5, 65.0, -0.5),
            count: 0,
            spread: Vec3::new(0.25, 0.0, 0.0),
            speed: 0.1,
        });
        let mut expected = vec![registry_id("minecraft:particle_type", "minecraft:item")];
        write_var(i32::from(wheat.0), &mut expected);
        expected.push(1);
        expected.extend_from_slice(&[0, 0]);
        expected.extend_from_slice(&[0, 0]);
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&65.0f64.to_be_bytes());
        expected.extend_from_slice(&(-0.5f64).to_be_bytes());
        expected.extend_from_slice(&0.25f32.to_be_bytes());
        expected.extend_from_slice(&0.0f32.to_be_bytes());
        expected.extend_from_slice(&0.0f32.to_be_bytes());
        expected.extend_from_slice(&0.1f32.to_be_bytes());
        expected.extend_from_slice(&0.1f32.to_be_bytes());
        expected.extend_from_slice(&0.1f32.to_be_bytes());
        expected.extend_from_slice(&[0, 0]);
        assert_eq!(encode(event), vec![(id::LEVEL_PARTICLES, expected)]);
    }

    #[test]
    fn titles_golden_bytes() {
        let times = ClientEvent::TitleTimes {
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        };
        assert_eq!(
            encode(times),
            vec![(
                id::SET_TITLES_ANIMATION,
                vec![0, 0, 0, 10, 0, 0, 0, 70, 0, 0, 0, 20]
            )]
        );
        assert_eq!(
            encode(ClientEvent::ClearTitles { reset: true }),
            vec![(id::CLEAR_TITLES, vec![1])]
        );
        let hi = vec![8, 0, 2, b'H', b'i'];
        assert_eq!(
            encode(ClientEvent::TitleText(Component::text("Hi"))),
            vec![(id::SET_TITLE_TEXT, hi.clone())]
        );
        assert_eq!(
            encode(ClientEvent::SubtitleText(Component::text("Hi"))),
            vec![(id::SET_SUBTITLE_TEXT, hi)]
        );
    }

    #[test]
    fn particle_data_golden_bytes() {
        let data = |name: &str, data: ParticleData| particle_options(name, &data).unwrap().data;
        let mut dust = vec![0x00, 0xFF, 0x00, 0x00];
        dust.extend_from_slice(&1.0f32.to_be_bytes());
        assert_eq!(
            data(
                "minecraft:dust",
                ParticleData::Dust {
                    color: 0xFF0000,
                    scale: 1.0
                }
            ),
            dust
        );
        assert_eq!(
            data(
                "minecraft:block",
                ParticleData::Block(rspigot_domain::BlockStateId(1))
            ),
            vec![1]
        );
        assert_eq!(
            data("minecraft:happy_villager", ParticleData::None),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn creeper_explosion_golden_bytes() {
        let event = ClientEvent::Explode {
            center: Vec3::new(0.5, 64.0, 2.5),
            radius: 3.0,
            block_count: 12,
            knockback: None,
            large: false,
            play_sound: true,
            gust: None,
        };
        let particle = |name: &str| registry_id("minecraft:particle_type", name);
        let sound = rspigot_data::registries::static_index(
            "minecraft:sound_event",
            "minecraft:entity.generic.explode",
        )
        .unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&64.0f64.to_be_bytes());
        expected.extend_from_slice(&2.5f64.to_be_bytes());
        expected.extend_from_slice(&3.0f32.to_be_bytes());
        expected.extend_from_slice(&12i32.to_be_bytes());
        expected.push(0);
        expected.push(particle("minecraft:explosion"));
        write_var(i32::try_from(sound).unwrap() + 1, &mut expected);
        expected.push(2);
        expected.push(particle("minecraft:poof"));
        expected.extend_from_slice(&0.5f32.to_be_bytes());
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.push(1);
        expected.push(particle("minecraft:smoke"));
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.push(1);
        expected.push(1);
        assert_eq!(encode(event), vec![(id::EXPLODE, expected)]);
    }

    #[test]
    fn a_wind_burst_carries_the_gust_particle_and_its_own_sound_without_block_particles_golden_bytes()
     {
        let event = ClientEvent::Explode {
            center: Vec3::new(0.5, 64.0, 2.5),
            radius: 3.0,
            block_count: 0,
            knockback: None,
            large: true,
            play_sound: true,
            gust: Some("minecraft:entity.breeze.wind_burst"),
        };
        let particle = |name: &str| registry_id("minecraft:particle_type", name);
        let sound = rspigot_data::registries::static_index(
            "minecraft:sound_event",
            "minecraft:entity.breeze.wind_burst",
        )
        .unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&64.0f64.to_be_bytes());
        expected.extend_from_slice(&2.5f64.to_be_bytes());
        expected.extend_from_slice(&3.0f32.to_be_bytes());
        expected.extend_from_slice(&0i32.to_be_bytes());
        expected.push(0);
        expected.push(particle("minecraft:gust_emitter_large"));
        write_var(i32::try_from(sound).unwrap() + 1, &mut expected);
        expected.push(0);
        expected.push(1);
        assert_eq!(encode(event), vec![(id::EXPLODE, expected)]);
    }

    #[test]
    fn arrow_spawn_carries_the_owner_id_golden_bytes() {
        let event = ClientEvent::SpawnEntity {
            entity: EntityId(12),
            uuid: uuid::Uuid::from_u128(2),
            kind: EntityKind::Other(registry_id("minecraft:entity_type", "minecraft:arrow").into()),
            position: Vec3::new(0.5, 65.5, 0.5),
            rotation: Rotation::new(0.0, 0.0),
            head_yaw: 0.0,
            data: 0x0102,
            velocity: Vec3::ZERO,
        };
        let mut expected = vec![12];
        expected.extend_from_slice(&0u64.to_be_bytes());
        expected.extend_from_slice(&2u64.to_be_bytes());
        expected.push(registry_id("minecraft:entity_type", "minecraft:arrow"));
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&65.5f64.to_be_bytes());
        expected.extend_from_slice(&0.5f64.to_be_bytes());
        expected.extend_from_slice(&[0, 0, 0, 0, 0x82, 0x02]);
        assert_eq!(encode(event), vec![(id::ADD_ENTITY, expected)]);
    }
}
