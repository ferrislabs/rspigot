use rspigot_core::plugins::{
    BlockSnapshot, BookMeta, BoundingBox, ChunkRef, DamageSourceView, DroppedItem, EnchantOffer,
    HolderRef, Input, InventoryOwner, InventoryRef, InventoryView, LecternData, Location,
    LootContext, MerchantRef, OwnerRef, PluginInfo, SenderView, SignData, SpawnerData, Trade,
    TrialSpawnerData,
};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{
    self, command_sender, holder_ref, inventory_ref, merchant_ref, owner_ref,
};

use super::player::{self, wire_player};
use super::shared::{
    self, block_pos, required, rotation, vec3, wire_block_pos, wire_item, wire_rotation, wire_vec,
};
use super::{Malformed, block, entity};

pub(super) fn wire_location(location: Location) -> v1::Location {
    v1::Location {
        world: location.world,
        position: Some(wire_vec(location.position)),
        rotation: Some(wire_rotation(location.rotation)),
    }
}

pub(super) fn location(location: v1::Location) -> Result<Location, Malformed> {
    Ok(Location {
        world: location.world,
        position: vec3(required(location.position)?),
        rotation: rotation(required(location.rotation)?),
    })
}

pub(super) fn wire_block_snapshot(snapshot: BlockSnapshot) -> v1::BlockSnapshot {
    v1::BlockSnapshot {
        pos: Some(wire_block_pos(snapshot.pos)),
        state: Some(block::wire_state(snapshot.state)),
    }
}

pub(super) fn wire_inventory_ref(inventory: InventoryRef) -> v1::InventoryRef {
    let (owner, plugin) = match inventory.owner {
        InventoryOwner::Custom { plugin, id } => (inventory_ref::Owner::Custom(id), plugin),
        InventoryOwner::Player(id) => (
            inventory_ref::Owner::Player(id.0.hyphenated().to_string()),
            String::new(),
        ),
        InventoryOwner::Block(pos) => (
            inventory_ref::Owner::Block(wire_block_pos(pos)),
            String::new(),
        ),
        InventoryOwner::Entity(id) => (inventory_ref::Owner::Entity(id.0), String::new()),
        InventoryOwner::Detached => (inventory_ref::Owner::Detached(v1::Empty {}), String::new()),
    };
    v1::InventoryRef {
        owner: Some(owner),
        size: inventory.size,
        kind: inventory.kind,
        plugin,
        id: inventory.id,
    }
}

pub(super) fn inventory_ref(inventory: v1::InventoryRef) -> Result<InventoryRef, Malformed> {
    let owner = match required(inventory.owner)? {
        inventory_ref::Owner::Custom(id) => InventoryOwner::Custom {
            plugin: inventory.plugin,
            id,
        },
        inventory_ref::Owner::Player(id) => InventoryOwner::Player(player::id(id)?),
        inventory_ref::Owner::Block(pos) => InventoryOwner::Block(block_pos(pos)),
        inventory_ref::Owner::Entity(id) => InventoryOwner::Entity(EntityId(id)),
        inventory_ref::Owner::Detached(_) => InventoryOwner::Detached,
    };
    Ok(InventoryRef {
        owner,
        size: inventory.size,
        kind: inventory.kind,
        id: inventory.id,
    })
}

pub(super) fn wire_inventory_view(view: InventoryView) -> v1::InventoryView {
    v1::InventoryView {
        top: Some(wire_inventory_ref(view.top)),
        bottom: Some(wire_inventory_ref(view.bottom)),
        title: view.title,
        player: Some(wire_player(view.player)),
    }
}

pub(super) fn wire_chunk_ref(chunk: ChunkRef) -> v1::ChunkRef {
    v1::ChunkRef {
        world: chunk.world,
        x: chunk.x,
        z: chunk.z,
    }
}

pub(super) fn wire_damage_source(source: DamageSourceView) -> v1::DamageSource {
    v1::DamageSource {
        kind: source.kind,
        causing: source.causing.map(entity::wire_view),
        direct: source.direct.map(entity::wire_view),
        location: source.location.map(wire_vec),
    }
}

pub(super) fn command_sender(sender: v1::CommandSender) -> Result<SenderView, Malformed> {
    Ok(match required(sender.sender)? {
        command_sender::Sender::Player(player) => SenderView::Player(player::snapshot(player)?),
        command_sender::Sender::Console(_) => SenderView::Console,
    })
}

pub(super) fn wire_command_sender(sender: SenderView) -> v1::CommandSender {
    let sender = match sender {
        SenderView::Player(player) => command_sender::Sender::Player(wire_player(player)),
        SenderView::Console => command_sender::Sender::Console(v1::Empty {}),
    };
    v1::CommandSender {
        sender: Some(sender),
    }
}

pub(super) fn wire_trade(trade: Trade) -> v1::Trade {
    v1::Trade {
        result: Some(wire_item(trade.result)),
        ingredients: trade.ingredients.into_iter().map(wire_item).collect(),
        uses: trade.uses,
        max_uses: trade.max_uses,
        experience_reward: trade.experience_reward,
        villager_experience: trade.villager_experience,
        price_multiplier: trade.price_multiplier,
        demand: trade.demand,
        special_price: trade.special_price,
    }
}

pub(super) fn trade(trade: v1::Trade) -> Result<Trade, Malformed> {
    Ok(Trade {
        result: shared::item(required(trade.result)?),
        ingredients: trade.ingredients.into_iter().map(shared::item).collect(),
        uses: trade.uses,
        max_uses: trade.max_uses,
        experience_reward: trade.experience_reward,
        villager_experience: trade.villager_experience,
        price_multiplier: trade.price_multiplier,
        demand: trade.demand,
        special_price: trade.special_price,
    })
}

pub(super) fn wire_plugin_info(plugin: PluginInfo) -> v1::PluginInfo {
    v1::PluginInfo {
        name: plugin.name,
        version: plugin.version,
    }
}

pub(super) const fn wire_bounding_box(bounds: BoundingBox) -> v1::BoundingBox {
    v1::BoundingBox {
        min: Some(wire_vec(bounds.min)),
        max: Some(wire_vec(bounds.max)),
    }
}

pub(super) const fn wire_input(input: Input) -> v1::Input {
    v1::Input {
        forward: input.forward,
        backward: input.backward,
        left: input.left,
        right: input.right,
        jump: input.jump,
        sneak: input.sneak,
        sprint: input.sprint,
    }
}

pub(super) fn wire_spawner_data(spawner: SpawnerData) -> v1::SpawnerData {
    v1::SpawnerData {
        pos: Some(wire_block_pos(spawner.pos)),
        spawned: spawner.spawned,
        delay: spawner.delay,
    }
}

pub(super) const fn wire_trial_spawner_data(spawner: TrialSpawnerData) -> v1::TrialSpawnerData {
    v1::TrialSpawnerData {
        pos: Some(wire_block_pos(spawner.pos)),
        ominous: spawner.ominous,
    }
}

pub(super) fn wire_lectern_data(lectern: LecternData) -> v1::LecternData {
    v1::LecternData {
        pos: Some(wire_block_pos(lectern.pos)),
        book: lectern.book.map(wire_item),
        page: lectern.page,
    }
}

pub(super) fn wire_dropped_item(dropped: DroppedItem) -> v1::DroppedItem {
    v1::DroppedItem {
        entity: Some(entity::wire_view(dropped.entity)),
        item: Some(wire_item(dropped.item)),
    }
}

pub(super) fn dropped_item(dropped: v1::DroppedItem) -> Result<DroppedItem, Malformed> {
    Ok(DroppedItem {
        entity: entity::view(required(dropped.entity)?)?,
        item: shared::item(required(dropped.item)?),
    })
}

pub(super) fn wire_sign_data(sign: SignData) -> v1::SignData {
    v1::SignData {
        pos: Some(wire_block_pos(sign.pos)),
        front: sign.front,
        back: sign.back,
        waxed: sign.waxed,
    }
}

pub(super) fn wire_enchant_offer(offer: EnchantOffer) -> v1::EnchantOffer {
    v1::EnchantOffer {
        enchantment: offer.enchantment,
        level: offer.level,
        cost: offer.cost,
    }
}

pub(super) fn wire_owner_ref(owner: OwnerRef) -> v1::OwnerRef {
    let owner = match owner {
        OwnerRef::Player(player) => owner_ref::Owner::Player(wire_player(player)),
        OwnerRef::Entity(view) => owner_ref::Owner::Entity(entity::wire_view(view)),
        OwnerRef::Offline(id) => owner_ref::Owner::Offline(id.hyphenated().to_string()),
    };
    v1::OwnerRef { owner: Some(owner) }
}

pub(super) fn wire_holder_ref(holder: HolderRef) -> v1::HolderRef {
    let holder = match holder {
        HolderRef::Player(player) => holder_ref::Holder::Player(wire_player(player)),
        HolderRef::Entity(view) => holder_ref::Holder::Entity(entity::wire_view(view)),
        HolderRef::Block(pos) => holder_ref::Holder::Block(wire_block_pos(pos)),
    };
    v1::HolderRef {
        holder: Some(holder),
    }
}

pub(super) fn wire_merchant_ref(merchant: MerchantRef) -> v1::MerchantRef {
    let merchant = match merchant {
        MerchantRef::Entity(view) => merchant_ref::Merchant::Entity(entity::wire_view(view)),
        MerchantRef::Custom => merchant_ref::Merchant::Custom(v1::Empty {}),
    };
    v1::MerchantRef {
        merchant: Some(merchant),
    }
}

pub(super) fn wire_book_meta(book: BookMeta) -> v1::BookMeta {
    v1::BookMeta {
        title: book.title,
        author: book.author,
        pages: book.pages,
    }
}

pub(super) fn book_meta(book: v1::BookMeta) -> Result<BookMeta, Malformed> {
    Ok(BookMeta {
        title: book.title,
        author: book.author,
        pages: book.pages,
    })
}

pub(super) fn wire_loot_context(context: LootContext) -> v1::LootContext {
    v1::LootContext {
        location: Some(wire_location(context.location)),
        luck: context.luck,
        killer: context.killer.map(entity::wire_view),
        looted: context.looted.map(entity::wire_view),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::PlayerSnapshot;
    use rspigot_domain::player::PlayerId;
    use rspigot_domain::{BlockPos, Rotation, Vec3};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn a_location_and_an_inventory_survive_the_round_trip() {
        let spot = Location {
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.5, 64.0, -3.0),
            rotation: Rotation::new(90.0, 10.0),
        };
        assert_eq!(location(wire_location(spot.clone())).ok(), Some(spot));
        for owner in [
            InventoryOwner::Custom {
                plugin: "shop".to_owned(),
                id: 7,
            },
            InventoryOwner::Player(PlayerId(Uuid::from_u128(3))),
            InventoryOwner::Block(BlockPos::new(1, 2, 3)),
            InventoryOwner::Entity(EntityId(9)),
            InventoryOwner::Detached,
        ] {
            let inventory = InventoryRef {
                owner,
                size: 27,
                kind: "CHEST".to_owned(),
                id: 12,
            };
            assert_eq!(
                inventory_ref(wire_inventory_ref(inventory.clone())).ok(),
                Some(inventory)
            );
        }
        assert!(location(v1::Location::default()).is_err());
        let stranger = v1::InventoryRef {
            owner: Some(inventory_ref::Owner::Player("nobody".to_owned())),
            ..v1::InventoryRef::default()
        };
        assert!(inventory_ref(stranger).is_err());
    }

    #[test]
    fn a_command_sender_survives_the_round_trip() {
        let steve = PlayerSnapshot {
            meta: Default::default(),
            permissions: Vec::new(),
            id: PlayerId(Uuid::from_u128(1)),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.0, 64.0, -3.5),
            rotation: Rotation::new(90.0, -30.0),
            game_mode: rspigot_domain::player::GameMode::Creative,
        };
        for sender in [SenderView::Player(steve), SenderView::Console] {
            assert_eq!(
                command_sender(wire_command_sender(sender.clone())).ok(),
                Some(sender)
            );
        }
        assert!(command_sender(v1::CommandSender::default()).is_err());
    }
}
