use std::sync::Arc;

use parking_lot::Mutex;
use rspigot_data::advancements;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Identifier};
use rspigot_items::ItemKind;

use super::permission::{LEVEL_GAMEMASTERS, may_use};
use super::slots::SlotHolder;
use super::tree::{CommandNodeKind, build_tree};
use super::{CommandContext, CommandOutcome, CommandSender, Target, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::ports::{DatapackError, DatapackSource, PackContents, PackFile, PackMeta};
use crate::server::Server;

const ROOT: &str = "minecraft:story/root";
const GEMS: &str = "{\"pools\":[{\"rolls\":1,\"entries\":[{\"type\":\"minecraft:item\",\"name\":\"minecraft:diamond\",\"functions\":[{\"function\":\"minecraft:set_count\",\"count\":3}]}]}]}";

#[derive(Default)]
struct MemoryPacks {
    packs: Mutex<Vec<PackContents>>,
}

impl DatapackSource for MemoryPacks {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.packs.lock().clone())
    }
}

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    let pack = PackContents {
        name: "gems".to_owned(),
        meta: PackMeta::default(),
        loot_tables: vec![PackFile {
            id: Identifier::parse("t:gems").unwrap(),
            text: GEMS.to_owned(),
        }],
        item_modifiers: vec![PackFile {
            id: Identifier::parse("t:triple").unwrap(),
            text: "{\"function\":\"minecraft:set_count\",\"count\":3}".to_owned(),
        }],
        ..PackContents::default()
    };
    let source = Arc::new(MemoryPacks {
        packs: Mutex::new(vec![pack]),
    });
    let mut server = server.with_datapacks(source);
    execute(
        &mut server,
        CommandSender::Player(id),
        "scoreboard objectives add n dummy",
    );
    server.start(std::time::Duration::from_secs(10));
    server.deliver();
    let rig = Rig { server, id, link };
    rig.link.take();
    rig
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn score(&self) -> Option<i32> {
        self.server
            .scoreboard()
            .score("Alice", "n")
            .map(|score| score.value)
    }

    fn slot(&self, slot: usize) -> Option<ItemStack> {
        self.server.players[&self.id].inventory.get(slot).cloned()
    }

    fn owned(&self, kind: ItemKind) -> u32 {
        self.server.players[&self.id]
            .inventory
            .slots()
            .iter()
            .flatten()
            .filter(|stack| stack.item == kind.id())
            .map(|stack| u32::from(stack.count.get()))
            .sum()
    }

    fn items_on_ground(&self) -> usize {
        self.server
            .select_entities(CommandSender::Player(self.id), "@e[type=item]")
            .expect("the selector parses")
            .len()
    }

    fn chest(&mut self, y: i32) -> BlockPos {
        assert_eq!(
            self.run(&format!("setblock 0 {y} 0 minecraft:chest")),
            CommandOutcome::Success
        );
        BlockPos::new(0, y, 0)
    }
}

fn first_text(rig: &Rig) -> String {
    rig.texts().into_iter().last().unwrap_or_default()
}

#[test]
fn granting_an_advancement_says_so_and_completes_it() {
    let mut rig = rig();
    assert_eq!(
        rig.run(&format!("advancement grant Alice only {ROOT}")),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.advancement.grant.one.to.one"));
    assert!(rig.server.done_advancements(rig.id).contains(&ROOT));
}

#[test]
fn granting_through_follows_the_parent_chain_and_the_children() {
    let mut rig = rig();
    let target = "minecraft:story/iron_tools";
    assert_eq!(
        rig.run(&format!("advancement grant Alice through {target}")),
        CommandOutcome::Success
    );
    let done = rig.server.done_advancements(rig.id);
    let mut parent = advancements::find(target).and_then(|(_, found)| found.parent);
    let mut chain = 0;
    while let Some(id) = parent {
        assert!(done.contains(&id), "{id}");
        chain += 1;
        parent = advancements::find(id).and_then(|(_, found)| found.parent);
    }
    assert!(chain >= 3);
    assert!(done.contains(&target));
    assert!(done.contains(&"minecraft:story/mine_diamond"));
    assert!(!done.contains(&"minecraft:story/follow_ender_eye") || done.contains(&target));
}

#[test]
fn granting_until_stops_at_the_advancement_and_from_starts_there() {
    let mut rig = rig();
    rig.run("advancement grant Alice until minecraft:story/mine_stone");
    let done = rig.server.done_advancements(rig.id);
    assert!(done.contains(&ROOT) && done.contains(&"minecraft:story/mine_stone"));
    assert!(!done.contains(&"minecraft:story/upgrade_tools"));
    let mut rig = rig_fresh();
    rig.run("advancement grant Alice from minecraft:story/mine_stone");
    let done = rig.server.done_advancements(rig.id);
    assert!(done.contains(&"minecraft:story/upgrade_tools"));
    assert!(!done.contains(&ROOT));
}

fn rig_fresh() -> Rig {
    rig()
}

#[test]
fn granting_everything_counts_many_advancements() {
    let mut rig = rig();
    assert_eq!(
        rig.run("advancement grant Alice everything"),
        CommandOutcome::Success
    );
    assert!(
        rig.texts()
            .iter()
            .any(|text| text.starts_with("commands.advancement.grant.many.to.one"))
    );
    assert!(rig.server.done_advancements(rig.id).len() > 100);
}

#[test]
fn granting_one_criterion_names_it_and_rejects_unknown_ones() {
    let mut rig = rig();
    assert_eq!(
        rig.run(&format!(
            "advancement grant Alice only {ROOT} crafting_table"
        )),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.advancement.grant.criterion.to.one"));
    assert_eq!(
        rig.run(&format!("advancement grant Alice only {ROOT} nope")),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.advancement.criterionNotFound"));
}

#[test]
fn revoking_resends_the_progress_and_a_second_revoke_changes_nothing() {
    let mut rig = rig();
    rig.run(&format!("advancement grant Alice only {ROOT}"));
    rig.texts();
    assert_eq!(
        rig.run(&format!("advancement revoke Alice only {ROOT}")),
        CommandOutcome::Success
    );
    let events = rig.link.take();
    let resent = events.iter().any(|event| {
        matches!(event, ClientEvent::Advancements { progress, .. }
            if progress.iter().any(|(id, criteria)| *id == ROOT
                && criteria.iter().all(|(_, when)| when.is_none())))
    });
    assert!(resent);
    assert!(!rig.server.done_advancements(rig.id).contains(&ROOT));
    assert_eq!(
        rig.run(&format!("advancement revoke Alice only {ROOT}")),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.advancement.revoke.one.to.one.failure"));
}

#[test]
fn advancement_commands_need_targets_and_known_advancements() {
    let mut rig = rig();
    assert_eq!(
        rig.run("advancement grant Nobody everything"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("argument.entity.notfound.player"));
    assert_eq!(
        rig.run("advancement grant Alice only minecraft:nope/none"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("argument.resource.not_found"));
}

#[test]
fn execute_store_receives_the_changed_advancement_count() {
    let mut rig = rig();
    rig.run(&format!(
        "execute store result score Alice n run advancement grant Alice only {ROOT}"
    ));
    assert_eq!(rig.score(), Some(1));
}

#[test]
fn recipe_give_unlocks_and_take_forgets() {
    let mut rig = rig();
    assert_eq!(
        rig.run("recipe give Alice minecraft:stick"),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.recipe.give.success.single 1 Alice"));
    assert!(
        rig.server
            .known_recipes(rig.id)
            .contains(&"minecraft:stick")
    );
    assert_eq!(
        rig.run("recipe give Alice minecraft:stick"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.recipe.give.failed"));
    assert_eq!(
        rig.run("recipe take Alice minecraft:stick"),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.recipe.take.success.single 1 Alice"));
    assert!(rig.server.known_recipes(rig.id).is_empty());
    assert_eq!(
        rig.run("recipe take Alice minecraft:stick"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.recipe.take.failed"));
}

#[test]
fn recipe_star_gives_every_recipe_and_unknown_ones_are_refused() {
    let mut rig = rig();
    assert_eq!(rig.run("recipe give Alice *"), CommandOutcome::Success);
    assert!(rig.server.known_recipes(rig.id).len() > 100);
    rig.texts();
    assert_eq!(
        rig.run("recipe give Alice minecraft:nope"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("recipe.notFound"));
    assert_eq!(rig.run("recipe give Nobody *"), CommandOutcome::Failure);
}

#[test]
fn item_replace_fills_the_hotbar_and_armor_slots() {
    let mut rig = rig();
    assert_eq!(
        rig.run("item replace entity Alice hotbar.0 with minecraft:stone 5"),
        CommandOutcome::Success
    );
    assert!(
        first_text(&rig)
            .starts_with("commands.item.entity.replace.success.single.known_item 1 Alice")
    );
    let stack = rig.slot(36).expect("a stack");
    assert_eq!((stack.item, stack.count.get()), (ItemKind::Stone.id(), 5));
    assert_eq!(
        rig.run("item replace entity Alice armor.head with minecraft:iron_helmet"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(5).map(|s| s.item), Some(ItemKind::IronHelmet.id()));
    assert_eq!(
        rig.run("item replace entity Alice weapon.offhand with minecraft:torch"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(45).map(|s| s.item), Some(ItemKind::Torch.id()));
}

#[test]
fn item_replace_on_a_chest_block_sets_the_container_slot() {
    let mut rig = rig();
    let pos = rig.chest(100);
    assert_eq!(
        rig.run("item replace block 0 100 0 container.3 with minecraft:diamond 2"),
        CommandOutcome::Success
    );
    assert!(
        first_text(&rig).starts_with("commands.item.block.replace.success.known_item 1 0 100 0")
    );
    let items = rig.server.block_items(pos, 27);
    let stack = items[3].as_ref().expect("a stack");
    assert_eq!((stack.item, stack.count.get()), (ItemKind::Diamond.id(), 2));
}

#[test]
fn item_replace_ranges_distribute_fill_cycles_and_override_clears() {
    let mut rig = rig();
    rig.run("item replace entity Alice hotbar.* with minecraft:dirt");
    assert_eq!(rig.slot(36).map(|s| s.item), Some(ItemKind::Dirt.id()));
    assert!(rig.slot(37).is_none());
    rig.run("item fill entity Alice hotbar.* with minecraft:dirt");
    assert_eq!(rig.slot(44).map(|s| s.item), Some(ItemKind::Dirt.id()));
    rig.run("item override entity Alice hotbar.* with minecraft:stone");
    assert_eq!(rig.slot(36).map(|s| s.item), Some(ItemKind::Stone.id()));
    assert!(rig.slot(37).is_none());
}

#[test]
fn item_replace_from_copies_another_slot() {
    let mut rig = rig();
    rig.run("item replace entity Alice hotbar.0 with minecraft:stone 7");
    assert_eq!(
        rig.run("item replace entity Alice hotbar.1 from entity Alice hotbar.0"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(37).map(|s| s.count.get()), Some(7));
    let pos = rig.chest(101);
    assert_eq!(
        rig.run("item replace block 0 101 0 container.0 from entity Alice hotbar.0"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.server.block_items(pos, 27)[0]
            .as_ref()
            .map(|s| s.count.get()),
        Some(7)
    );
}

#[test]
fn item_modify_accepts_a_datapack_item_modifier_id() {
    let mut rig = rig();
    rig.run("item replace entity Alice hotbar.0 with minecraft:stone 5");
    rig.texts();
    assert_eq!(
        rig.run("item modify entity Alice hotbar.0 t:triple"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(36).map(|s| s.count.get()), Some(3));
    assert_eq!(
        rig.run("item modify entity Alice hotbar.0 t:missing"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("argument.resource.not_found t:missing"));
}

#[test]
fn item_replace_from_applies_a_modifier_id() {
    let mut rig = rig();
    rig.run("item replace entity Alice hotbar.1 with minecraft:stone 5");
    assert_eq!(
        rig.run("item replace entity Alice hotbar.0 from entity Alice hotbar.1 t:triple"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(36).map(|s| s.count.get()), Some(3));
}

#[test]
fn item_modifier_ids_are_completed() {
    let rig = rig();
    let names = rig
        .server
        .game_command_names(2, "item modify entity Alice hotbar.0")
        .expect("completed");
    assert!(names.contains(&"t:triple".to_owned()));
}

#[test]
fn item_modify_applies_set_count() {
    let mut rig = rig();
    rig.run("item replace entity Alice hotbar.0 with minecraft:stone 5");
    rig.texts();
    assert_eq!(
        rig.run(
            "item modify entity Alice hotbar.0 {\"function\":\"minecraft:set_count\",\"count\":2}"
        ),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.item.entity.modify.success.single 1 Alice"));
    assert_eq!(rig.slot(36).map(|s| s.count.get()), Some(2));
}

#[test]
fn item_errors_name_the_problem() {
    let mut rig = rig();
    assert_eq!(
        rig.run("item replace entity Alice nope.0 with minecraft:stone"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("slot.unknown"));
    assert_eq!(
        rig.run("item replace entity Alice container.50 with minecraft:stone"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.item.target.no_such_slot"));
    assert_eq!(
        rig.run("item replace block 0 100 0 container.0 with minecraft:stone"),
        CommandOutcome::Failure
    );
    assert!(first_text(&rig).starts_with("commands.item.target.not_a_container"));
    assert_eq!(
        rig.run("item replace entity Nobody hotbar.0 with minecraft:stone"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run(
            "item modify entity Alice hotbar.0 {\"function\":\"minecraft:set_count\",\"count\":2}"
        ),
        CommandOutcome::Failure
    );
    let text = first_text(&rig);
    assert!(text.starts_with("commands.item.target.failed"), "{text}");
    assert_eq!(
        rig.run("item modify entity Alice hotbar.0 {\"function\":\"minecraft:set_lore\"}"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("item modify entity Alice hotbar.0 minecraft:unknown_modifier"),
        CommandOutcome::Failure
    );
}

#[test]
fn loot_give_hands_a_datapack_table_to_the_player() {
    let mut rig = rig();
    assert_eq!(
        rig.run("loot give Alice loot t:gems"),
        CommandOutcome::Success
    );
    assert_eq!(rig.owned(ItemKind::Diamond), 3);
    assert!(first_text(&rig).starts_with("commands.drop.success.single_with_table 3"));
}

#[test]
fn loot_results_feed_execute_store() {
    let mut rig = rig();
    rig.run("execute store result score Alice n run loot give Alice loot t:gems");
    assert_eq!(rig.score(), Some(1));
}

#[test]
fn loot_mine_uses_the_tool_and_the_block() {
    let mut rig = rig();
    rig.run("setblock 0 100 0 minecraft:diamond_ore");
    assert_eq!(
        rig.run("loot give Alice mine 0 100 0 minecraft:iron_pickaxe"),
        CommandOutcome::Success
    );
    assert_eq!(rig.owned(ItemKind::Diamond), 1);
    rig.run("setblock 0 100 0 minecraft:air");
    assert_eq!(
        rig.run("loot give Alice mine 0 100 0 minecraft:iron_pickaxe"),
        CommandOutcome::Failure
    );
}

#[test]
fn loot_spawn_creates_item_entities() {
    let mut rig = rig();
    let before = rig.items_on_ground();
    assert_eq!(
        rig.run("loot spawn 0 100 0 loot t:gems"),
        CommandOutcome::Success
    );
    assert_eq!(rig.items_on_ground(), before + 1);
}

#[test]
fn loot_insert_and_replace_fill_containers() {
    let mut rig = rig();
    let pos = rig.chest(102);
    assert_eq!(
        rig.run("loot insert 0 102 0 loot t:gems"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.server.block_items(pos, 27)[0]
            .as_ref()
            .map(|s| s.count.get()),
        Some(3)
    );
    assert_eq!(
        rig.run("loot replace block 0 102 0 container.5 loot t:gems"),
        CommandOutcome::Success
    );
    assert!(rig.server.block_items(pos, 27)[5].is_some());
    assert_eq!(
        rig.run("loot replace entity Alice hotbar.2 loot t:gems"),
        CommandOutcome::Success
    );
    assert_eq!(rig.slot(38).map(|s| s.item), Some(ItemKind::Diamond.id()));
    assert_eq!(
        rig.run("loot insert 0 110 0 loot t:gems"),
        CommandOutcome::Failure
    );
}

#[test]
fn loot_kill_rolls_the_entity_table_and_unknown_tables_fail() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 0 100 0");
    assert_eq!(
        rig.run("loot give Alice kill @e[type=cow,limit=1]"),
        CommandOutcome::Success
    );
    assert!(rig.owned(ItemKind::Beef) >= 1);
    assert_eq!(
        rig.run("loot give Alice loot t:missing"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("loot give Alice fish t:gems 0 100 0 mainhand"),
        CommandOutcome::Success
    );
}

#[test]
fn the_waypoint_list_names_the_transmitters_and_modify_edits_their_icon() {
    let mut rig = rig();
    assert_eq!(rig.run("waypoint list"), CommandOutcome::Success);
    assert!(first_text(&rig).starts_with("commands.waypoint.list.success 1"));
    assert_eq!(
        rig.run("waypoint modify Alice color red"),
        CommandOutcome::Success
    );
    assert!(first_text(&rig).starts_with("commands.waypoint.modify.color"));
    assert_eq!(
        rig.run("waypoint modify Alice color nope"),
        CommandOutcome::Failure
    );
}

#[test]
fn stopwatches_are_created_queried_restarted_and_removed() {
    let mut rig = rig();
    assert_eq!(rig.run("stopwatch query t:w"), CommandOutcome::Failure);
    assert!(first_text(&rig).starts_with("commands.stopwatch.does_not_exist"));
    assert_eq!(rig.run("stopwatch create t:w"), CommandOutcome::Success);
    assert!(first_text(&rig).starts_with("commands.stopwatch.create.success"));
    assert_eq!(rig.run("stopwatch create t:w"), CommandOutcome::Failure);
    assert!(first_text(&rig).starts_with("commands.stopwatch.already_exists"));
    let old = rig.server.stopwatch_started("t:w").unwrap() - 5_000;
    rig.server.set_stopwatch("t:w", Some(old));
    rig.run("execute store result score Alice n run stopwatch query t:w 2");
    assert!(rig.score().is_some_and(|value| (10..=11).contains(&value)));
    assert_eq!(rig.run("stopwatch restart t:w"), CommandOutcome::Success);
    assert!(rig.server.stopwatch_started("t:w").unwrap() > old);
    assert_eq!(rig.run("stopwatch remove t:w"), CommandOutcome::Success);
    assert_eq!(rig.server.stopwatch_started("t:w"), None);
    assert_eq!(rig.run("stopwatch remove t:w"), CommandOutcome::Failure);
}

#[test]
fn the_commands_need_game_master_level() {
    for name in [
        "advancement",
        "recipe",
        "item",
        "loot",
        "waypoint",
        "stopwatch",
    ] {
        assert!(!may_use(name, LEVEL_GAMEMASTERS - 1), "{name}");
        assert!(may_use(name, LEVEL_GAMEMASTERS), "{name}");
    }
}

#[test]
fn tab_completion_offers_the_main_arguments() {
    let rig = rig();
    let names = |head: &str| rig.server.game_command_names(2, head).expect("completed");
    assert!(names("advancement grant Alice only").contains(&ROOT.to_owned()));
    assert!(
        names("advancement grant Alice only minecraft:story/root")
            .contains(&"crafting_table".to_owned())
    );
    assert!(names("advancement grant").contains(&"Alice".to_owned()));
    assert!(names("advancement").contains(&"revoke".to_owned()));
    assert!(names("recipe give Alice").contains(&"*".to_owned()));
    assert!(names("recipe give Alice").contains(&"minecraft:stick".to_owned()));
    assert!(names("item replace entity Alice").contains(&"hotbar.0".to_owned()));
    assert!(names("item replace entity Alice hotbar.0").contains(&"with".to_owned()));
    assert!(
        names("loot give Alice loot")
            .iter()
            .any(|n| n.starts_with("minecraft:"))
    );
    assert!(names("loot give Alice").contains(&"mine".to_owned()));
    assert!(names("stopwatch").contains(&"restart".to_owned()));
    assert!(rig.server.game_command_names(2, "give Alice").is_none());
}

#[test]
fn locate_completion_offers_structures_and_their_tags() {
    let rig = rig();
    let names = rig
        .server
        .game_command_names(2, "locate structure")
        .expect("completed");
    assert!(names.contains(&"minecraft:stronghold".to_owned()));
    assert!(names.contains(&"#minecraft:village".to_owned()));
    assert!(
        rig.server
            .game_command_names(2, "locate")
            .expect("completed")
            .contains(&"poi".to_owned())
    );
}

fn literals(tree: &super::CommandTree, node: usize) -> Vec<String> {
    let mut names: Vec<String> = tree.nodes[node]
        .children
        .iter()
        .filter_map(|child| match &tree.nodes[*child].kind {
            CommandNodeKind::Literal(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names
}

fn child(tree: &super::CommandTree, node: usize, wanted: &str) -> usize {
    tree.nodes[node]
        .children
        .iter()
        .copied()
        .find(|child| match &tree.nodes[*child].kind {
            CommandNodeKind::Literal(name) => name == wanted,
            CommandNodeKind::Argument { name, .. } => name == wanted,
            CommandNodeKind::Root => false,
        })
        .unwrap_or_else(|| panic!("{wanted}"))
}

#[test]
fn the_tree_matches_the_vanilla_nodes() {
    let tree = build_tree(2);
    let at = |path: &[&str]| {
        path.iter()
            .fold(tree.root, |node, step| child(&tree, node, step))
    };
    assert_eq!(literals(&tree, at(&["advancement"])), ["grant", "revoke"]);
    assert_eq!(
        literals(&tree, at(&["advancement", "grant", "targets"])),
        ["everything", "from", "only", "through", "until"]
    );
    assert!(
        tree.nodes[at(&[
            "advancement",
            "revoke",
            "targets",
            "only",
            "advancement",
            "criterion"
        ])]
        .executable
    );
    assert_eq!(literals(&tree, at(&["recipe", "take", "targets"])), ["*"]);
    assert!(tree.nodes[at(&["recipe", "give", "targets", "recipe"])].executable);
    assert_eq!(
        literals(&tree, at(&["item"])),
        ["fill", "modify", "override", "replace"]
    );
    assert_eq!(
        literals(&tree, at(&["item", "replace", "entity", "target", "slots"])),
        ["from", "with"]
    );
    assert!(tree.nodes[at(&["item", "modify", "block", "target", "slots", "modifier"])].executable);
    assert!(
        tree.nodes[at(&[
            "item",
            "fill",
            "entity",
            "target",
            "slots",
            "from",
            "entity",
            "source",
            "sourceSlots",
            "modifier"
        ])]
        .executable
    );
    assert!(
        tree.nodes[at(&[
            "item", "override", "block", "target", "slots", "with", "item", "count"
        ])]
        .executable
    );
    assert_eq!(
        literals(&tree, at(&["loot"])),
        ["give", "insert", "replace", "spawn"]
    );
    assert_eq!(
        literals(&tree, at(&["loot", "give", "players"])),
        ["fish", "kill", "loot", "mine"]
    );
    assert_eq!(
        literals(
            &tree,
            at(&["loot", "replace", "entity", "entities", "slot", "count"])
        ),
        ["fish", "kill", "loot", "mine"]
    );
    assert_eq!(
        literals(&tree, at(&["loot", "spawn", "targetPos", "mine", "pos"])),
        ["mainhand", "offhand"]
    );
    assert_eq!(literals(&tree, at(&["waypoint"])), ["list", "modify"]);
    assert_eq!(
        literals(&tree, at(&["waypoint", "modify", "waypoint"])),
        ["color", "style"]
    );
    assert_eq!(
        literals(&tree, at(&["stopwatch"])),
        ["create", "query", "remove", "restart"]
    );
    assert!(tree.nodes[at(&["stopwatch", "query", "id", "scale"])].executable);
    let low = build_tree(1);
    assert!(literals(&low, low.root).iter().all(|name| name != "loot"));
}

#[test]
fn slot_access_reads_what_it_wrote() {
    let mut rig = rig();
    let holder = SlotHolder::Entity(Target::Player(rig.id));
    let stack = ItemStack::new(ItemKind::Stone.id(), std::num::NonZeroU8::new(3).unwrap());
    assert!(rig.server.write_slot(holder, 200, Some(stack.clone())));
    assert_eq!(rig.server.read_slot(holder, 200), Some(Some(stack)));
    assert_eq!(rig.server.read_slot(holder, 40), None);
}
