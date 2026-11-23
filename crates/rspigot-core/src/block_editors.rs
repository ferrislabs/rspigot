use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_nbt::Compound;

use crate::chunks::WorldQuery;
use crate::commands::CommandSender;
use crate::events::ClientEvent;
use crate::locate::{PlaceKind, QueryKind};
use crate::ports::{ClientLink, PlaceRequest, TemplateMirror, TemplateRotation};
use crate::redstone::{face_named, step};
use crate::server::{Audience, Server};

const JIGSAW: &str = "jigsaw";
const STRUCTURE: &str = "structure_block";
const MAX_LEVELS: i32 = 20;
const SCAN_RADIUS: i32 = 80;
const MIN_SCAN_SPAN: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JigsawEdit {
    pub pos: BlockPos,
    pub name: String,
    pub target: String,
    pub pool: String,
    pub final_state: String,
    pub joint: String,
    pub selection_priority: i32,
    pub placement_priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureUpdate {
    UpdateData,
    SaveArea,
    LoadArea,
    ScanArea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureMode {
    Save,
    Load,
    Corner,
    Data,
}

impl StructureMode {
    const fn name(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Load => "load",
            Self::Corner => "corner",
            Self::Data => "data",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureEdit {
    pub pos: BlockPos,
    pub update: StructureUpdate,
    pub mode: StructureMode,
    pub name: String,
    pub offset: [i32; 3],
    pub size: [i32; 3],
    pub mirror: TemplateMirror,
    pub rotation: TemplateRotation,
    pub data: String,
    pub integrity: f32,
    pub seed: i64,
    pub ignore_entities: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub strict: bool,
}

const fn rotation_name(rotation: TemplateRotation) -> &'static str {
    match rotation {
        TemplateRotation::None => "NONE",
        TemplateRotation::Clockwise90 => "CLOCKWISE_90",
        TemplateRotation::Clockwise180 => "CLOCKWISE_180",
        TemplateRotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
    }
}

const fn mirror_name(mirror: TemplateMirror) -> &'static str {
    match mirror {
        TemplateMirror::None => "NONE",
        TemplateMirror::LeftRight => "LEFT_RIGHT",
        TemplateMirror::FrontBack => "FRONT_BACK",
    }
}

struct TemplateLoad {
    name: String,
    at: BlockPos,
    rotation: TemplateRotation,
    mirror: TemplateMirror,
    integrity: f32,
    seed: i32,
}

fn rotation_of(name: Option<&str>) -> TemplateRotation {
    match name {
        Some("CLOCKWISE_90") => TemplateRotation::Clockwise90,
        Some("CLOCKWISE_180") => TemplateRotation::Clockwise180,
        Some("COUNTERCLOCKWISE_90") => TemplateRotation::Counterclockwise90,
        _ => TemplateRotation::None,
    }
}

fn mirror_of(name: Option<&str>) -> TemplateMirror {
    match name {
        Some("LEFT_RIGHT") => TemplateMirror::LeftRight,
        Some("FRONT_BACK") => TemplateMirror::FrontBack,
        _ => TemplateMirror::None,
    }
}

fn structure_data(old: Option<&Compound>, edit: &StructureEdit) -> Compound {
    let mut data = old.cloned().unwrap_or_default();
    data.insert("name", edit.name.as_str());
    data.insert("metadata", edit.data.as_str());
    data.insert("posX", edit.offset[0]);
    data.insert("posY", edit.offset[1]);
    data.insert("posZ", edit.offset[2]);
    data.insert("sizeX", edit.size[0]);
    data.insert("sizeY", edit.size[1]);
    data.insert("sizeZ", edit.size[2]);
    data.insert("rotation", rotation_name(edit.rotation));
    data.insert("mirror", mirror_name(edit.mirror));
    data.insert("mode", edit.mode.name().to_uppercase());
    data.insert("integrity", edit.integrity);
    data.insert("seed", edit.seed);
    data.insert("ignoreEntities", edit.ignore_entities);
    data.insert("showair", edit.show_air);
    data.insert("showboundingbox", edit.show_bounding_box);
    data.insert("strict", edit.strict);
    data
}

fn jigsaw_data(old: Option<&Compound>, edit: &JigsawEdit) -> Compound {
    let mut data = old.cloned().unwrap_or_default();
    data.insert("name", edit.name.as_str());
    data.insert("target", edit.target.as_str());
    data.insert("pool", edit.pool.as_str());
    data.insert("final_state", edit.final_state.as_str());
    data.insert("joint", edit.joint.as_str());
    data.insert("selection_priority", edit.selection_priority);
    data.insert("placement_priority", edit.placement_priority);
    data
}

impl<L: ClientLink> Server<L> {
    fn editable_block(&self, id: PlayerId, pos: BlockPos, kind: BlockKind) -> bool {
        self.can_use_game_master_blocks(id)
            && self.world.chunks.get(pos.chunk()).is_some()
            && self.block_or_air(pos).kind() == Some(kind)
    }

    fn block_entity_compound(&self, pos: BlockPos, kind: &str) -> Option<&Compound> {
        let wanted = format!("minecraft:{kind}");
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .filter(|entity| entity.kind.as_str() == wanted)
            .map(|entity| &entity.data)
    }

    fn store_block_entity(&mut self, pos: BlockPos, kind: &'static str, data: Compound) {
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(kind),
                data: data.clone(),
            });
        }
        self.push(
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockEntityUpdate { pos, kind, data },
        );
    }

    pub(crate) fn handle_set_jigsaw_block(&mut self, id: PlayerId, edit: &JigsawEdit) {
        if !self.editable_block(id, edit.pos, BlockKind::Jigsaw) {
            return;
        }
        let data = jigsaw_data(self.block_entity_compound(edit.pos, JIGSAW), edit);
        self.store_block_entity(edit.pos, JIGSAW, data);
    }

    pub(crate) fn handle_jigsaw_generate(&mut self, id: PlayerId, pos: BlockPos, levels: i32) {
        if !self.editable_block(id, pos, BlockKind::Jigsaw) {
            return;
        }
        let Some(data) = self.block_entity_compound(pos, JIGSAW) else {
            return;
        };
        let pool = data.get_str("pool").unwrap_or("minecraft:empty").to_owned();
        let target = data
            .get_str("target")
            .unwrap_or("minecraft:empty")
            .to_owned();
        let front = self
            .block_or_air(pos)
            .property("orientation")
            .and_then(|value| value.split('_').next())
            .and_then(face_named);
        let Some(front) = front else {
            return;
        };
        let at = step(pos, front);
        let request = PlaceRequest::Jigsaw {
            pool: pool.clone(),
            target,
            max_depth: levels.clamp(1, MAX_LEVELS),
            pos: at,
        };
        self.begin_world_query(
            CommandSender::Player(id),
            WorldQuery::Place(request),
            QueryKind::Place(PlaceKind::Jigsaw, at),
            pool,
        );
    }

    pub(crate) fn handle_set_structure_block(&mut self, id: PlayerId, edit: &StructureEdit) {
        if !self.editable_block(id, edit.pos, BlockKind::StructureBlock) {
            return;
        }
        let state = self.block_or_air(edit.pos);
        if let Some(next) = state.with_property("mode", edit.mode.name())
            && next != state
        {
            self.set_block_quiet(edit.pos, next);
        }
        let data = structure_data(self.block_entity_compound(edit.pos, STRUCTURE), edit);
        self.store_block_entity(edit.pos, STRUCTURE, data);
        if edit.name.trim().is_empty() {
            return;
        }
        let key = match edit.update {
            StructureUpdate::UpdateData => return,
            StructureUpdate::SaveArea => "structure_block.save_failure",
            StructureUpdate::ScanArea => {
                self.scan_structure(id, edit);
                return;
            }
            StructureUpdate::LoadArea => {
                self.load_structure(id, edit);
                return;
            }
        };
        self.push(
            Audience::One(id),
            ClientEvent::system(Component::translatable(
                key,
                vec![Component::text(edit.name.as_str())],
            )),
        );
    }

    fn matching_corners(&self, origin: BlockPos, name: &str) -> Vec<BlockPos> {
        let mut found = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| {
                        entity.kind.as_str() == "minecraft:structure_block"
                            && entity.data.get_str("mode") == Some("CORNER")
                            && entity.data.get_str("name") == Some(name)
                            && (entity.pos.x - origin.x).abs() <= SCAN_RADIUS
                            && (entity.pos.z - origin.z).abs() <= SCAN_RADIUS
                    })
                    .map(|entity| entity.pos),
            );
        }
        found
    }

    fn scan_structure(&mut self, id: PlayerId, edit: &StructureEdit) {
        let corners = if edit.mode == StructureMode::Save {
            self.matching_corners(edit.pos, edit.name.as_str())
        } else {
            Vec::new()
        };
        let (mut low, mut high) = (edit.pos, edit.pos);
        for corner in &corners {
            low = BlockPos::new(
                low.x.min(corner.x),
                low.y.min(corner.y),
                low.z.min(corner.z),
            );
            high = BlockPos::new(
                high.x.max(corner.x),
                high.y.max(corner.y),
                high.z.max(corner.z),
            );
        }
        let measured = !corners.is_empty()
            && high.x - low.x > MIN_SCAN_SPAN
            && high.y - low.y > MIN_SCAN_SPAN
            && high.z - low.z > MIN_SCAN_SPAN;
        if measured {
            let mut data = self
                .block_entity_compound(edit.pos, STRUCTURE)
                .cloned()
                .unwrap_or_default();
            data.insert("posX", low.x - edit.pos.x + 1);
            data.insert("posY", low.y - edit.pos.y + 1);
            data.insert("posZ", low.z - edit.pos.z + 1);
            data.insert("sizeX", high.x - low.x - 1);
            data.insert("sizeY", high.y - low.y - 1);
            data.insert("sizeZ", high.z - low.z - 1);
            self.store_block_entity(edit.pos, STRUCTURE, data);
        }
        let key = if measured {
            "structure_block.size_success"
        } else {
            "structure_block.size_failure"
        };
        self.push(
            Audience::One(id),
            ClientEvent::system(Component::translatable(
                key,
                vec![Component::text(edit.name.as_str())],
            )),
        );
    }

    fn load_structure(&mut self, id: PlayerId, edit: &StructureEdit) {
        let at = BlockPos::new(
            edit.pos.x + edit.offset[0],
            edit.pos.y + edit.offset[1],
            edit.pos.z + edit.offset[2],
        );
        self.begin_template_load(
            CommandSender::Player(id),
            TemplateLoad {
                name: edit.name.clone(),
                at,
                rotation: edit.rotation,
                mirror: edit.mirror,
                integrity: edit.integrity,
                seed: edit.seed as i32,
            },
        );
    }

    fn begin_template_load(&mut self, sender: CommandSender, load: TemplateLoad) {
        let request = PlaceRequest::Template {
            id: load.name.clone(),
            pos: load.at,
            rotation: load.rotation,
            mirror: load.mirror,
            integrity: load.integrity,
            seed: load.seed,
        };
        self.begin_world_query(
            sender,
            WorldQuery::Place(request),
            QueryKind::Place(PlaceKind::Template, load.at),
            load.name,
        );
    }

    pub(crate) fn structure_block_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let powered = self.has_neighbor_signal(pos);
        let Some(mut data) = self.block_entity_compound(pos, STRUCTURE).cloned() else {
            return;
        };
        if data.get_bool("powered").unwrap_or(false) == powered {
            return;
        }
        data.insert("powered", powered);
        self.store_block_entity(pos, STRUCTURE, data.clone());
        let name = data.get_str("name").unwrap_or_default().trim().to_owned();
        if !powered || name.is_empty() || state.property("mode") != Some("load") {
            return;
        }
        let at = BlockPos::new(
            pos.x + data.get_i32("posX").unwrap_or(0),
            pos.y + data.get_i32("posY").unwrap_or(0),
            pos.z + data.get_i32("posZ").unwrap_or(0),
        );
        self.begin_template_load(
            CommandSender::Console,
            TemplateLoad {
                name,
                at,
                rotation: rotation_of(data.get_str("rotation")),
                mirror: mirror_of(data.get_str("mirror")),
                integrity: data.get_f64("integrity").map_or(1.0, |value| value as f32),
                seed: data.get_i64("seed").unwrap_or(0) as i32,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;

    use super::*;
    use crate::end_tests::{Tap, arrive};
    use crate::events::PlayerInput;
    use rspigot_domain::player::GameMode;

    const POS: BlockPos = BlockPos::new(2, 70, 2);

    fn rig(creative_op: bool) -> (Server<Tap>, PlayerId, Tap) {
        let (mut server, id, link) = arrive();
        if let Some(player) = server.players.get_mut(&id) {
            player.operator = creative_op;
            player.op_level = if creative_op { 4 } else { 0 };
            player.game_mode = if creative_op {
                GameMode::Creative
            } else {
                GameMode::Survival
            };
        }
        for _ in 0..5 {
            server.tick();
        }
        server.deliver();
        link.take();
        (server, id, link)
    }

    fn jigsaw_edit() -> JigsawEdit {
        JigsawEdit {
            pos: POS,
            name: "minecraft:a".to_owned(),
            target: "minecraft:b".to_owned(),
            pool: "minecraft:village/plains/houses".to_owned(),
            final_state: "minecraft:air".to_owned(),
            joint: "aligned".to_owned(),
            selection_priority: 1,
            placement_priority: 2,
        }
    }

    fn structure_edit(update: StructureUpdate) -> StructureEdit {
        StructureEdit {
            pos: POS,
            update,
            mode: StructureMode::Load,
            name: "minecraft:igloo/top".to_owned(),
            offset: [1, 2, 3],
            size: [4, 5, 6],
            mirror: TemplateMirror::LeftRight,
            rotation: TemplateRotation::Clockwise90,
            data: "meta".to_owned(),
            integrity: 0.5,
            seed: 9,
            ignore_entities: true,
            show_air: true,
            show_bounding_box: false,
            strict: true,
        }
    }

    fn place(server: &mut Server<Tap>, kind: BlockKind, props: &[(&str, &str)]) {
        let state = kind.state_with(props).unwrap_or(kind.default_state());
        server.set_block(POS, state);
        server.deliver();
    }

    #[test]
    fn an_operator_in_creative_mode_stores_the_jigsaw_fields() {
        let (mut server, id, link) = rig(true);
        place(&mut server, BlockKind::Jigsaw, &[]);
        link.take();
        server.handle_input(id, PlayerInput::SetJigsawBlock(Box::new(jigsaw_edit())));
        server.deliver();
        let data = server
            .block_entity_compound(POS, JIGSAW)
            .expect("jigsaw entity")
            .clone();
        assert_eq!(
            data.get_str("pool"),
            Some("minecraft:village/plains/houses")
        );
        assert_eq!(data.get_str("joint"), Some("aligned"));
        assert_eq!(data.get_i32("placement_priority"), Some(2));
        assert!(
            link.take().iter().any(|event| matches!(
                event,
                ClientEvent::BlockEntityUpdate { kind: "jigsaw", .. }
            ))
        );
    }

    #[test]
    fn a_non_operator_cannot_edit_a_jigsaw_or_a_structure_block() {
        let (mut server, id, link) = rig(false);
        place(&mut server, BlockKind::Jigsaw, &[]);
        link.take();
        server.handle_input(id, PlayerInput::SetJigsawBlock(Box::new(jigsaw_edit())));
        server.handle_input(
            id,
            PlayerInput::JigsawGenerate {
                pos: POS,
                levels: 3,
                keep_jigsaws: false,
            },
        );
        place(&mut server, BlockKind::StructureBlock, &[]);
        server.handle_input(
            id,
            PlayerInput::SetStructureBlock(Box::new(structure_edit(StructureUpdate::LoadArea))),
        );
        server.deliver();
        assert!(server.block_entity_compound(POS, JIGSAW).is_none());
        assert!(server.block_entity_compound(POS, STRUCTURE).is_none());
        assert!(server.pending_queries.is_empty());
        assert!(
            !link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::BlockEntityUpdate { .. }))
        );
    }

    #[test]
    fn the_structure_block_keeps_its_fields_and_its_mode_property() {
        let (mut server, id, _link) = rig(true);
        place(&mut server, BlockKind::StructureBlock, &[("mode", "save")]);
        server.handle_input(
            id,
            PlayerInput::SetStructureBlock(Box::new(structure_edit(StructureUpdate::UpdateData))),
        );
        let data = server
            .block_entity_compound(POS, STRUCTURE)
            .expect("structure entity");
        assert_eq!(data.get_str("name"), Some("minecraft:igloo/top"));
        assert_eq!(data.get_i32("posZ"), Some(3));
        assert_eq!(data.get_i32("sizeY"), Some(5));
        assert_eq!(data.get_str("rotation"), Some("CLOCKWISE_90"));
        assert_eq!(data.get_str("mirror"), Some("LEFT_RIGHT"));
        assert_eq!(data.get_str("mode"), Some("LOAD"));
        assert_eq!(data.get_str("metadata"), Some("meta"));
        assert_eq!(server.block_or_air(POS).property("mode"), Some("load"));
        assert!(server.pending_queries.is_empty());
    }

    #[test]
    fn loading_a_structure_starts_a_template_placement_and_saving_reports_failure() {
        let (mut server, id, link) = rig(true);
        place(&mut server, BlockKind::StructureBlock, &[]);
        link.take();
        server.handle_input(
            id,
            PlayerInput::SetStructureBlock(Box::new(structure_edit(StructureUpdate::LoadArea))),
        );
        assert_eq!(server.pending_queries.len(), 1);
        server.handle_input(
            id,
            PlayerInput::SetStructureBlock(Box::new(structure_edit(StructureUpdate::SaveArea))),
        );
        server.deliver();
        let reported = link.take().into_iter().any(|event| match event {
            ClientEvent::SystemChat { message, .. } => message
                .plain_text()
                .starts_with("structure_block.save_failure"),
            _ => false,
        });
        assert!(reported);
    }

    fn corner(server: &mut Server<Tap>, at: BlockPos, name: &str) {
        server.set_block(
            at,
            BlockKind::StructureBlock
                .state_with(&[("mode", "corner")])
                .unwrap_or(BlockKind::StructureBlock.default_state()),
        );
        server.store_block_entity(
            at,
            STRUCTURE,
            Compound::new().with("name", name).with("mode", "CORNER"),
        );
    }

    fn scan_message(link: &Tap) -> Option<String> {
        link.take().into_iter().find_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => {
                let text = message.plain_text();
                text.starts_with("structure_block.size").then_some(text)
            }
            _ => None,
        })
    }

    fn scan(server: &mut Server<Tap>, id: PlayerId) {
        let mut edit = structure_edit(StructureUpdate::ScanArea);
        edit.mode = StructureMode::Save;
        server.handle_input(id, PlayerInput::SetStructureBlock(Box::new(edit)));
        server.deliver();
    }

    #[test]
    fn scanning_a_save_block_measures_the_box_to_the_corner_blocks_of_the_same_name() {
        let (mut server, id, link) = rig(true);
        place(&mut server, BlockKind::StructureBlock, &[("mode", "save")]);
        corner(&mut server, POS.offset(5, 4, 5), "minecraft:igloo/top");
        corner(&mut server, POS.offset(-3, 9, 1), "minecraft:other");
        link.take();
        scan(&mut server, id);
        let data = server
            .block_entity_compound(POS, STRUCTURE)
            .expect("structure entity");
        assert_eq!(
            [
                data.get_i32("posX"),
                data.get_i32("posY"),
                data.get_i32("posZ")
            ],
            [Some(1), Some(1), Some(1)]
        );
        assert_eq!(
            [
                data.get_i32("sizeX"),
                data.get_i32("sizeY"),
                data.get_i32("sizeZ")
            ],
            [Some(4), Some(3), Some(4)]
        );
        assert!(
            scan_message(&link)
                .is_some_and(|text| text.starts_with("structure_block.size_success"))
        );
    }

    #[test]
    fn scanning_with_no_matching_corner_reports_a_failure_and_keeps_the_size() {
        let (mut server, id, link) = rig(true);
        place(&mut server, BlockKind::StructureBlock, &[("mode", "save")]);
        corner(&mut server, POS.offset(5, 4, 5), "minecraft:other");
        link.take();
        scan(&mut server, id);
        let data = server
            .block_entity_compound(POS, STRUCTURE)
            .expect("structure entity");
        assert_eq!(data.get_i32("sizeX"), Some(4));
        assert_eq!(data.get_i32("sizeY"), Some(5));
        assert!(
            scan_message(&link)
                .is_some_and(|text| text.starts_with("structure_block.size_failure"))
        );
    }

    #[test]
    fn jigsaw_generate_starts_a_placement_from_the_block_front() {
        let (mut server, id, _link) = rig(true);
        place(
            &mut server,
            BlockKind::Jigsaw,
            &[("orientation", "east_up")],
        );
        server.handle_input(id, PlayerInput::SetJigsawBlock(Box::new(jigsaw_edit())));
        server.handle_input(
            id,
            PlayerInput::JigsawGenerate {
                pos: POS,
                levels: 3,
                keep_jigsaws: true,
            },
        );
        assert_eq!(server.pending_queries.len(), 1);
    }
}
