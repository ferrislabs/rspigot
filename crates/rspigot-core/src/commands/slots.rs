use rspigot_domain::BlockPos;

use super::dispatch::Target;

pub(crate) const MAINHAND: i32 = 98;
pub(crate) const OFFHAND: i32 = 99;
pub(crate) const FEET: i32 = 100;
pub(crate) const LEGS: i32 = 101;
pub(crate) const CHEST: i32 = 102;
pub(crate) const HEAD: i32 = 103;
pub(crate) const BODY: i32 = 105;
pub(crate) const SADDLE: i32 = 106;
pub(crate) const HORSE_CHEST: i32 = 498;
pub(crate) const CURSOR: i32 = 499;
pub(crate) const ENDER_BASE: i32 = 200;
pub(crate) const MOB_BASE: i32 = 300;
pub(crate) const HORSE_BASE: i32 = 500;
pub(crate) const CRAFTING_BASE: i32 = 500;

const CONTAINER: i32 = 54;
const HOTBAR: i32 = 9;
const INVENTORY: i32 = 27;
const ENDER: i32 = 27;
const MOB: i32 = 8;
const HORSE: i32 = 15;
const CRAFTING: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotHolder {
    Entity(Target),
    Block(BlockPos),
}

fn counted(base: i32, size: i32) -> Vec<i32> {
    (base..base + size).collect()
}

const ARMOR: [i32; 5] = [HEAD, CHEST, LEGS, FEET, BODY];

fn numbered(name: &str, prefix: &str, base: i32, size: i32) -> Option<Vec<i32>> {
    let tail = name.strip_prefix(prefix)?;
    if tail == "*" {
        return Some(counted(base, size));
    }
    let index: i32 = tail.parse().ok()?;
    (0..size)
        .contains(&index)
        .then(|| vec![base + index])
        .filter(|_| tail == index.to_string())
}

pub(crate) fn slot_ids(name: &str) -> Option<Vec<i32>> {
    if let Some(found) = [
        ("container.", 0, CONTAINER),
        ("hotbar.", 0, HOTBAR),
        ("inventory.", 9, INVENTORY),
        ("enderchest.", ENDER_BASE, ENDER),
        ("mob.inventory.", MOB_BASE, MOB),
        ("horse.", HORSE_BASE, HORSE),
        ("player.crafting.", CRAFTING_BASE, CRAFTING),
    ]
    .iter()
    .find_map(|(prefix, base, size)| numbered(name, prefix, *base, *size))
    {
        return Some(found);
    }
    Some(match name {
        "contents" => counted(0, CONTAINER),
        "weapon" | "weapon.mainhand" => vec![MAINHAND],
        "weapon.offhand" => vec![OFFHAND],
        "weapon.*" => vec![MAINHAND, OFFHAND],
        "armor.head" => vec![HEAD],
        "armor.chest" => vec![CHEST],
        "armor.legs" => vec![LEGS],
        "armor.feet" => vec![FEET],
        "armor.body" => vec![BODY],
        "armor.*" => ARMOR.to_vec(),
        "saddle" => vec![SADDLE],
        "horse.chest" => vec![HORSE_CHEST],
        "player.cursor" => vec![CURSOR],
        _ => return None,
    })
}

pub(crate) fn is_single(name: &str) -> bool {
    name != "contents" && !name.ends_with('*')
}

pub(crate) fn names() -> Vec<String> {
    let mut out = vec!["contents".to_owned()];
    for (prefix, size) in [
        ("container.", CONTAINER),
        ("hotbar.", HOTBAR),
        ("inventory.", INVENTORY),
        ("enderchest.", ENDER),
        ("mob.inventory.", MOB),
        ("horse.", HORSE),
        ("player.crafting.", CRAFTING),
    ] {
        out.extend((0..size).map(|index| format!("{prefix}{index}")));
        out.push(format!("{prefix}*"));
    }
    out.extend(
        [
            "weapon",
            "weapon.mainhand",
            "weapon.offhand",
            "weapon.*",
            "armor.head",
            "armor.chest",
            "armor.legs",
            "armor.feet",
            "armor.body",
            "armor.*",
            "saddle",
            "horse.chest",
            "player.cursor",
        ]
        .map(str::to_owned),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names_map_to_their_ids() {
        assert_eq!(slot_ids("container.5"), Some(vec![5]));
        assert_eq!(slot_ids("inventory.0"), Some(vec![9]));
        assert_eq!(slot_ids("enderchest.26"), Some(vec![226]));
        assert_eq!(slot_ids("hotbar.*").map(|ids| ids.len()), Some(9));
        assert_eq!(slot_ids("container.54"), None);
        assert_eq!(slot_ids("container.05"), None);
        assert_eq!(slot_ids("hotbar.9"), None);
    }

    #[test]
    fn named_slots_map_to_equipment_ids() {
        assert_eq!(slot_ids("weapon"), Some(vec![MAINHAND]));
        assert_eq!(slot_ids("weapon.offhand"), Some(vec![OFFHAND]));
        assert_eq!(slot_ids("armor.feet"), Some(vec![FEET]));
        assert_eq!(slot_ids("armor.*").map(|ids| ids.len()), Some(5));
        assert_eq!(slot_ids("player.cursor"), Some(vec![CURSOR]));
        assert_eq!(slot_ids("nope"), None);
    }

    #[test]
    fn only_wildcards_and_contents_are_ranges() {
        assert!(is_single("container.0"));
        assert!(is_single("weapon.mainhand"));
        assert!(!is_single("container.*"));
        assert!(!is_single("contents"));
    }

    #[test]
    fn every_listed_name_resolves() {
        for name in names() {
            assert!(slot_ids(&name).is_some(), "{name}");
        }
    }
}
