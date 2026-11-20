use rspigot_plugin::prelude::*;
use rspigot_plugin::{
    Access, ArgumentKind, ChoiceArgument, CommandArgument, CommandSpec, EventKind, GreedyText,
    HandlerSpec, Invocation, Listeners,
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
        choices: &[],
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Food,
    Tool,
}

impl ChoiceArgument for Kind {
    const CHOICES: &'static [&'static str] = &["food", "tool"];

    fn from_choice(choice: &str) -> Option<Self> {
        match choice {
            "food" => Some(Self::Food),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

#[derive(Plugin)]
struct Market;

#[listeners]
impl Market {
    #[command("shop", aliases = ["s", "store"])]
    async fn shop(&self, ctx: Context) {
        let _ = ctx;
    }

    #[command("shop buy", permission = "shop.buy")]
    async fn buy(&self, ctx: Context, item: Material, amount: u32) {
        let _ = (ctx, item, amount);
    }

    #[command("shop list")]
    async fn list(&self, ctx: Context, kind: Option<Kind>) {
        let _ = (ctx, kind);
    }

    #[command("probe")]
    async fn probe(
        &self,
        ctx: Context,
        a: f64,
        b: f32,
        who: Vec<Entity>,
        spot: Position,
        #[suggest(Self::notes)] note: GreedyText,
    ) {
        let _ = (ctx, a, b, who, spot, note);
    }

    async fn on_command(&self, invocation: Invocation) {
        let _ = invocation;
    }
}

impl Market {
    async fn notes(&self, player: &Player, typed: &str) -> Vec<String> {
        let _ = (player, typed);
        Vec::new()
    }
}

#[test]
fn a_name_with_words_is_a_command_with_literal_sub_commands_and_aliases() {
    let commands = Market::commands();
    assert_eq!(commands[0].name, "shop");
    assert_eq!(commands[0].aliases, ["s", "store"]);
    assert!(commands[0].arguments.is_empty());
    assert_eq!(commands[1].name, "shop");
    assert_eq!(commands[1].permission, Some("shop.buy"));
    assert_eq!(
        commands[1].arguments,
        vec![
            argument("buy", ArgumentKind::Literal, false, false),
            argument("item", ArgumentKind::Item, false, false),
            argument("amount", ArgumentKind::Integer, false, false),
        ]
    );
    assert_eq!(commands[2].arguments[0].name, "list");
    assert!(commands[2].aliases.is_empty());
}

#[test]
fn choices_and_the_other_kinds_follow_the_argument_types() {
    let commands = Market::commands();
    let kind = &commands[2].arguments[1];
    assert_eq!(kind.kind, ArgumentKind::Choice);
    assert_eq!(kind.choices, ["food", "tool"]);
    assert!(kind.optional);
    let kinds: Vec<ArgumentKind> = commands[3].arguments.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            ArgumentKind::Double,
            ArgumentKind::Float,
            ArgumentKind::Entities,
            ArgumentKind::Location,
            ArgumentKind::GreedyText
        ]
    );
}

#[test]
fn a_plugin_with_an_on_command_hook_and_a_suggestion_still_declares_one_decider() {
    let tab = Market::handlers()
        .into_iter()
        .filter(|spec| spec.event == EventKind::TabComplete)
        .count();
    assert_eq!(tab, 1);
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
                aliases: &[],
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
                aliases: &[],
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
