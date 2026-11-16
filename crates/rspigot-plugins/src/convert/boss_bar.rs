use rspigot_core::plugins::{BossBarOrder, BossBarSpec, BossBarView, Order};
use rspigot_plugin_protocol::v1::{self, boss_bar_order::Order as Wire};

use super::Malformed;
use super::shared::required;

pub(super) fn order(order: v1::BossBarOrder) -> Result<Order, Malformed> {
    let bar = order.bar;
    let order = match required(order.order)? {
        Wire::SetTitle(title) => BossBarOrder::SetTitle(title),
        Wire::SetColor(color) => BossBarOrder::SetColor(color),
        Wire::SetStyle(style) => BossBarOrder::SetStyle(style),
        Wire::SetProgress(progress) => BossBarOrder::SetProgress(progress),
        Wire::SetFlag(v1::BossFlagOrder { flag, enabled }) => {
            BossBarOrder::SetFlag { flag, enabled }
        }
        Wire::AddPlayer(player) => BossBarOrder::AddPlayer(player),
        Wire::RemovePlayer(player) => BossBarOrder::RemovePlayer(player),
        Wire::RemoveAllPlayers(_) => BossBarOrder::RemoveAllPlayers,
        Wire::SetVisible(visible) => BossBarOrder::SetVisible(visible),
        Wire::Remove(_) => BossBarOrder::Remove,
    };
    Ok(Order::BossBar { bar, order })
}

pub(super) fn spec(create: v1::BossBarCreate) -> BossBarSpec {
    BossBarSpec {
        title: create.title,
        color: create.color,
        style: create.style,
        flags: create.flags,
        key: create.key,
    }
}

pub(super) fn wire_state(view: BossBarView) -> v1::BossBarState {
    v1::BossBarState {
        bar: view.bar,
        key: view.key,
        title: view.title,
        color: view.color,
        style: view.style,
        progress: view.progress,
        flags: view.flags,
        visible: view.visible,
        players: view.players,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wired(order: Wire) -> v1::BossBarOrder {
        v1::BossBarOrder {
            bar: 4,
            order: Some(order),
        }
    }

    #[test]
    fn an_order_converts_with_its_bar() {
        assert_eq!(
            order(wired(Wire::SetProgress(0.5))).unwrap(),
            Order::BossBar {
                bar: 4,
                order: BossBarOrder::SetProgress(0.5)
            }
        );
        assert_eq!(
            order(wired(Wire::SetFlag(v1::BossFlagOrder {
                flag: "darken_screen".to_owned(),
                enabled: true
            })))
            .unwrap(),
            Order::BossBar {
                bar: 4,
                order: BossBarOrder::SetFlag {
                    flag: "darken_screen".to_owned(),
                    enabled: true
                }
            }
        );
        assert!(matches!(
            order(wired(Wire::Remove(v1::Empty {}))).unwrap(),
            Order::BossBar {
                order: BossBarOrder::Remove,
                ..
            }
        ));
    }

    #[test]
    fn an_empty_order_is_malformed() {
        assert!(
            order(v1::BossBarOrder {
                bar: 1,
                order: None
            })
            .is_err()
        );
    }

    #[test]
    fn the_creation_spec_and_state_convert_field_by_field() {
        let created = spec(v1::BossBarCreate {
            title: "T".to_owned(),
            color: "red".to_owned(),
            style: "progress".to_owned(),
            flags: vec!["create_world_fog".to_owned()],
            key: Some("a:b".to_owned()),
        });
        assert_eq!(created.key.as_deref(), Some("a:b"));
        assert_eq!(created.flags, ["create_world_fog"]);
        let state = wire_state(BossBarView {
            bar: 2,
            key: None,
            title: "T".to_owned(),
            color: "red".to_owned(),
            style: "progress".to_owned(),
            progress: 0.25,
            flags: Vec::new(),
            visible: true,
            players: vec!["p".to_owned()],
        });
        assert_eq!(state.progress, 0.25);
        assert_eq!(state.players, ["p"]);
        assert!(state.key.is_none());
    }
}
