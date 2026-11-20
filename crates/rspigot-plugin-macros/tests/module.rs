use rspigot_plugin::prelude::*;
use rspigot_plugin::{Call, Invocation, Outcome};

#[derive(Default)]
struct Joins;

#[listeners]
impl Joins {
    async fn on_player_join(&self, event: &PlayerJoin) {
        let _ = event;
    }

    async fn on_player_leave(&self, event: &PlayerLeave) {
        let _ = event;
    }

    #[command("home set")]
    async fn set(&self, ctx: Context) {
        let _ = ctx;
    }
}

#[derive(Default)]
struct Moves;

#[listeners]
impl Moves {
    #[priority(High)]
    async fn on_player_move(&self, event: &mut PlayerMove) {
        event.cancel();
    }

    #[command("home go")]
    async fn go(&self, ctx: Context, #[suggest(Self::homes)] name: String) {
        let _ = (ctx, name);
    }
}

impl Moves {
    async fn homes(&self, player: &Player, typed: &str) -> Vec<String> {
        let _ = (player, typed);
        Vec::new()
    }
}

#[derive(Default, Module)]
#[module(own)]
struct Node {
    #[module]
    joins: Joins,
    #[module]
    moves: Moves,
}

#[listeners]
impl Node {
    async fn welcome(&self, event: &PlayerJoin) {
        let _ = event;
    }

    #[command("home list")]
    async fn list(&self, ctx: Context) {
        let _ = ctx;
    }
}

#[derive(Default, Plugin)]
#[plugin(name = "tree", version = "0.3.0")]
struct Tree {
    #[module]
    node: Node,
    #[module]
    again: Joins,
}

#[derive(Default, Module)]
struct Pair(#[module] Joins, #[module] Joins);

struct Quiet;

#[listeners]
impl Quiet {}

#[derive(Plugin)]
#[plugin(own)]
struct Rooted {
    #[module]
    quiet: Quiet,
}

#[listeners]
impl Rooted {
    async fn on_player_join(&self, event: &PlayerJoin) {
        let _ = event;
    }
}

fn ids(specs: &[HandlerSpec]) -> Vec<(u32, EventKind)> {
    specs.iter().map(|spec| (spec.id, spec.event)).collect()
}

use rspigot_plugin::{EventKind, HandlerSpec};

#[test]
fn a_leaf_keeps_its_own_ids_and_slots() {
    assert_eq!(
        ids(&Joins::handlers()),
        vec![(1, EventKind::PlayerJoin), (2, EventKind::PlayerLeave)]
    );
    assert_eq!(<Joins as Listeners>::HANDLER_SLOTS, 2);
    assert_eq!(<Joins as Listeners>::COMMAND_SLOTS, 3);
    const { assert!(!<Joins as Listeners>::TAB_SLOT) };
    const { assert!(<Moves as Listeners>::TAB_SLOT) };
    assert_eq!(<Moves as Listeners>::HANDLER_SLOTS, 2);
    assert_eq!(<Moves as Listeners>::COMMAND_SLOTS, 2);
    let tab: Vec<u32> = Moves::handlers()
        .iter()
        .filter(|spec| spec.event == EventKind::TabComplete)
        .map(|spec| spec.id)
        .collect();
    assert_eq!(tab, [u32::MAX]);
}

#[test]
fn a_node_numbers_its_own_handlers_first_then_each_child_after_the_one_before() {
    assert_eq!(<Node as Listeners>::HANDLER_SLOTS, 1 + 2 + 2);
    assert_eq!(<Node as Listeners>::COMMAND_SLOTS, 2 + 3 + 2);
    assert_eq!(
        ids(&Node::handlers()),
        vec![
            (1, EventKind::PlayerJoin),
            (2, EventKind::PlayerJoin),
            (3, EventKind::PlayerLeave),
            (4, EventKind::PlayerMove),
            (5, EventKind::TabComplete),
        ]
    );
    const { assert!(!<Node as Listeners>::TAB_SLOT) };
}

#[test]
fn a_tree_numbers_by_offset_at_every_level() {
    let handlers = Tree::handlers();
    let all: Vec<u32> = handlers.iter().map(|spec| spec.id).collect();
    assert_eq!(all, vec![1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(<Tree as Listeners>::HANDLER_SLOTS, 5 + 2);
    assert_eq!(Tree::NAME, "tree");
    assert_eq!(Tree::VERSION, "0.3.0");
}

#[test]
fn command_ids_follow_the_same_offsets() {
    let commands = Tree::commands();
    let found: Vec<(u32, &str, &str)> = commands
        .iter()
        .map(|spec| (spec.id, spec.name, spec.arguments[0].name))
        .collect();
    assert_eq!(
        found,
        vec![
            (2, "home", "list"),
            (5, "home", "set"),
            (7, "home", "go"),
            (10, "home", "set"),
        ]
    );
}

#[test]
fn a_tuple_struct_is_a_node_and_two_fields_of_one_type_are_two_modules() {
    assert_eq!(
        ids(&Pair::handlers()),
        vec![
            (1, EventKind::PlayerJoin),
            (2, EventKind::PlayerLeave),
            (3, EventKind::PlayerJoin),
            (4, EventKind::PlayerLeave),
        ]
    );
}

#[test]
fn a_plugin_declared_own_with_modules_composes_its_own_handlers_first() {
    assert_eq!(ids(&Rooted::handlers()), vec![(1, EventKind::PlayerJoin)]);
    assert_eq!(<Rooted as Listeners>::HANDLER_SLOTS, 1);
}

#[test]
fn a_node_with_default_modules_derives_default() {
    let tree = Tree::default();
    assert_eq!(Tree::handlers().len(), 7);
    let _ = tree;
}

struct ByHand;

impl Listeners for ByHand {
    fn handlers() -> Vec<HandlerSpec> {
        Vec::new()
    }

    async fn dispatch(&self, call: Call) -> Outcome {
        let _ = call;
        Outcome::Observed
    }

    async fn enable(&self, server: &Server) {
        let _ = server;
    }

    async fn disable(&self) {}

    async fn invoke(&self, invocation: Invocation) {
        let _ = invocation;
    }
}

impl Plugin for ByHand {
    const NAME: &'static str = "by-hand";
    const VERSION: &'static str = "1";
}

#[test]
fn a_hand_written_listeners_impl_still_compiles_as_a_root() {
    assert!(ByHand::handlers().is_empty());
    assert_eq!(ByHand::NAME, "by-hand");
}
