use rspigot_plugin::prelude::*;
use rspigot_plugin::{
    Access, ArgumentKind, CommandArgument, CommandSpec, EventKind, HandlerSpec, Listeners,
};

#[derive(Plugin)]
struct Shop;

#[listeners]
impl Shop {
    #[command("give", permission = "kit.use")]
    async fn give(&self, ctx: Context, target: Player, item: Material, count: Option<u32>) {
        let _ = (ctx, target, item, count);
    }

    #[command("warp")]
    async fn warp(&self, ctx: Context, #[suggest(Self::places)] place: String, hard: bool) {
        let _ = (ctx, place, hard);
    }

    async fn on_player_join(&self, event: &PlayerJoin) {
        let _ = event;
    }
}

impl Shop {
    async fn places(&self, player: &Player, typed: &str) -> Vec<String> {
        let _ = (player, typed);
        Vec::new()
    }
}

fn argument(
    name: &'static str,
    kind: ArgumentKind,
    optional: bool,
    suggested: bool,
) -> CommandArgument {
    CommandArgument {
        name,
        kind,
        optional,
        suggested,
    }
}

#[test]
fn argument_types_give_the_command_tree() {
    let commands = Shop::commands();
    assert_eq!(
        commands,
        vec![
            CommandSpec {
                id: 1,
                name: "give",
                permission: Some("kit.use"),
                arguments: vec![
                    argument("target", ArgumentKind::Player, false, false),
                    argument("item", ArgumentKind::Item, false, false),
                    argument("count", ArgumentKind::Integer, true, false),
                ],
            },
            CommandSpec {
                id: 2,
                name: "warp",
                permission: None,
                arguments: vec![
                    argument("place", ArgumentKind::Text, false, true),
                    argument("hard", ArgumentKind::Boolean, false, false),
                ],
            },
        ]
    );
}

#[test]
fn a_suggesting_command_registers_one_tab_complete_decider() {
    let handlers = Shop::handlers();
    let tab: Vec<&HandlerSpec> = handlers
        .iter()
        .filter(|spec| spec.event == EventKind::TabComplete)
        .collect();
    assert_eq!(tab.len(), 1);
    assert_eq!(tab[0].access, Access::Decide);
    assert!(
        handlers
            .iter()
            .any(|spec| spec.event == EventKind::PlayerJoin && spec.access == Access::Observe)
    );
}

#[test]
fn a_plugin_without_commands_declares_none() {
    #[derive(Plugin)]
    struct Quiet;

    #[listeners]
    impl Quiet {}

    assert!(Quiet::commands().is_empty());
    assert!(Quiet::handlers().is_empty());
}

#[test]
fn commands_that_break_the_rules_do_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
