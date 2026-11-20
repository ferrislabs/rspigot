use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};

use crate::component::{self, ComponentValue};
use crate::item_meta::{
    self as meta, AttributeModifier, BannerLayer, Equippable, FireworkExplosion, Fireworks,
    ItemFlag, LodestoneTracker, PotionContents, Profile, Rarity, Tool, Trim, UseCooldown,
    WrittenBook,
};
use crate::{Color, CustomModelData, DyeColor, Enchantment, Error, ItemStack, Material, MetaValue};

pub(crate) const CUSTOM_DATA: &str = "minecraft:custom_data";
pub(crate) const PUBLIC_VALUES: &str = "PublicBukkitValues";
const TYPE: &str = "t";
const VALUE: &str = "v";

fn value_tag(value: &MetaValue) -> Tag {
    let (kind, held) = match value {
        MetaValue::Bool(flag) => ("bool", Tag::Byte(i8::from(*flag))),
        MetaValue::Int(number) => ("int", Tag::Long(*number)),
        MetaValue::Double(number) => ("double", Tag::Double(*number)),
        MetaValue::String(text) => ("string", Tag::String(text.clone())),
        MetaValue::Bytes(bytes) => ("bytes", Tag::ByteArray(bytes.clone())),
        MetaValue::List(items) => ("list", Tag::List(items.iter().map(value_tag).collect())),
        MetaValue::Map(entries) => (
            "map",
            Tag::Compound(
                entries
                    .iter()
                    .map(|(key, held)| (key.clone(), value_tag(held)))
                    .collect(),
            ),
        ),
    };
    Tag::Compound(Compound::new().with(TYPE, kind).with(VALUE, held))
}

fn typed_value(tag: &Tag) -> Option<MetaValue> {
    let compound = tag.as_compound()?;
    if compound.len() != 2 {
        return None;
    }
    let kind = compound.get_str(TYPE)?;
    let held = compound.get(VALUE)?;
    Some(match (kind, held) {
        ("bool", Tag::Byte(flag)) => MetaValue::Bool(*flag != 0),
        ("int", Tag::Long(number)) => MetaValue::Int(*number),
        ("double", Tag::Double(number)) => MetaValue::Double(*number),
        ("string", Tag::String(text)) => MetaValue::String(text.clone()),
        ("bytes", Tag::ByteArray(bytes)) => MetaValue::Bytes(bytes.clone()),
        ("list", Tag::List(items)) => {
            MetaValue::List(items.iter().map(typed_value).collect::<Option<_>>()?)
        }
        ("map", Tag::Compound(entries)) => MetaValue::Map(
            entries
                .iter()
                .map(|(key, held)| Some((key.to_owned(), typed_value(held)?)))
                .collect::<Option<BTreeMap<_, _>>>()?,
        ),
        _ => return None,
    })
}

fn foreign_value(tag: &Tag) -> Option<MetaValue> {
    match tag {
        Tag::Byte(_) | Tag::Short(_) | Tag::Int(_) | Tag::Long(_) => {
            tag.as_i64().map(MetaValue::Int)
        }
        Tag::Float(number) => Some(MetaValue::Double(f64::from(*number))),
        Tag::Double(number) => Some(MetaValue::Double(*number)),
        Tag::String(text) => Some(MetaValue::String(text.clone())),
        Tag::ByteArray(bytes) => Some(MetaValue::Bytes(bytes.clone())),
        _ => None,
    }
}

fn read_value(tag: &Tag) -> Option<MetaValue> {
    typed_value(tag).or_else(|| foreign_value(tag))
}

fn public_values(components: &Compound) -> Option<&Compound> {
    components
        .get_compound(CUSTOM_DATA)?
        .get_compound(PUBLIC_VALUES)
}

fn data_keys(components: &Compound) -> Vec<String> {
    public_values(components)
        .map(|values| {
            let mut keys: Vec<String> = values
                .iter()
                .filter(|(_, tag)| read_value(tag).is_some())
                .map(|(key, _)| key.to_owned())
                .collect();
            keys.sort();
            keys
        })
        .unwrap_or_default()
}

fn data_get(components: &Compound, key: &str) -> Option<MetaValue> {
    public_values(components)?.get(key).and_then(read_value)
}

fn data_edit(components: &mut Compound, change: impl FnOnce(&mut Compound)) {
    let mut data = components
        .get_compound(CUSTOM_DATA)
        .cloned()
        .unwrap_or_default();
    let mut values = data
        .get_compound(PUBLIC_VALUES)
        .cloned()
        .unwrap_or_default();
    change(&mut values);
    if values.is_empty() {
        data.remove(PUBLIC_VALUES);
    } else {
        data.insert(PUBLIC_VALUES, values);
    }
    if data.is_empty() {
        components.remove(CUSTOM_DATA);
    } else {
        components.insert(CUSTOM_DATA, data);
    }
}

fn custom_model_tag(data: &CustomModelData) -> Tag {
    let mut out = Compound::new();
    if !data.floats.is_empty() {
        out.insert(
            "floats",
            Tag::List(data.floats.iter().map(|value| Tag::Float(*value)).collect()),
        );
    }
    if !data.flags.is_empty() {
        out.insert(
            "flags",
            Tag::List(data.flags.iter().map(|value| Tag::from(*value)).collect()),
        );
    }
    if !data.strings.is_empty() {
        out.insert(
            "strings",
            Tag::List(
                data.strings
                    .iter()
                    .map(|value| Tag::from(value.as_str()))
                    .collect(),
            ),
        );
    }
    if !data.colors.is_empty() {
        out.insert(
            "colors",
            Tag::List(data.colors.iter().map(|value| Tag::Int(*value)).collect()),
        );
    }
    Tag::Compound(out)
}

fn custom_model_from(tag: &Tag) -> Option<CustomModelData> {
    let data = tag.as_compound()?;
    let list = |key: &str| data.get_list(key).unwrap_or_default();
    Some(CustomModelData {
        floats: list("floats")
            .iter()
            .filter_map(|value| value.as_f64().map(|value| value as f32))
            .collect(),
        flags: list("flags")
            .iter()
            .filter_map(|value| value.as_i64().map(|value| value != 0))
            .collect(),
        strings: list("strings")
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        colors: list("colors")
            .iter()
            .filter_map(|value| value.as_i64().and_then(|value| i32::try_from(value).ok()))
            .collect(),
    })
}

fn valid_key(key: &str) -> bool {
    let Some((namespace, path)) = key.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'))
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'))
}

impl ItemStack {
    fn compound(&self) -> Compound {
        component::decode(&self.components).unwrap_or_default()
    }

    fn edit(mut self, change: impl FnOnce(&mut Compound)) -> Self {
        let mut components = self.compound();
        change(&mut components);
        self.components = component::encode(&components);
        self
    }

    fn put(self, key: &str, tag: Tag) -> Self {
        self.edit(|components| {
            components.insert(key, tag);
        })
    }

    fn drop_component(self, key: &str) -> Self {
        self.edit(|components| {
            components.remove(key);
        })
    }

    fn switch(self, key: &str, on: bool, tag: Tag) -> Self {
        if on {
            self.put(key, tag)
        } else {
            self.drop_component(key)
        }
    }

    pub fn meta(&self) -> ItemMeta {
        ItemMeta {
            components: self.compound(),
        }
    }

    pub fn component(&self, key: &str) -> Option<ComponentValue> {
        self.compound().get(key).map(ComponentValue::from_tag)
    }

    pub fn component_keys(&self) -> Vec<String> {
        self.compound()
            .iter()
            .map(|(key, _)| key.to_owned())
            .collect()
    }

    pub fn set_component(&mut self, key: &str, value: ComponentValue) -> Result<(), Error> {
        let key = key.strip_prefix('!').unwrap_or(key);
        if !valid_key(key) {
            return Err(Error::Malformed(format!("`{key}` is not a component key")));
        }
        if meta::TYPED_ELSEWHERE.contains(&key) {
            return Err(Error::Malformed(format!(
                "`{key}` is set through the name, lore, enchant or custom_model_data of the stack"
            )));
        }
        let mut components = self.compound();
        components.insert(key, value.to_tag());
        self.components = component::encode(&components);
        Ok(())
    }

    pub fn with_component(mut self, key: &str, value: ComponentValue) -> Result<Self, Error> {
        self.set_component(key, value)?;
        Ok(self)
    }

    pub fn remove_component(&mut self, key: &str) -> bool {
        let mut components = self.compound();
        let had = components.remove(key).is_some();
        if had {
            self.components = component::encode(&components);
        }
        had
    }

    pub fn unbreakable(self, on: bool) -> Self {
        self.switch(meta::UNBREAKABLE, on, Tag::Compound(Compound::new()))
    }

    pub fn damage(self, damage: i32) -> Self {
        self.put(meta::DAMAGE, Tag::Int(damage))
    }

    pub fn max_damage(self, damage: i32) -> Self {
        self.put(meta::MAX_DAMAGE, Tag::Int(damage))
    }

    pub fn repair_cost(self, cost: i32) -> Self {
        self.put(meta::REPAIR_COST, Tag::Int(cost))
    }

    pub fn max_stack_size(self, size: u8) -> Self {
        self.put(meta::MAX_STACK_SIZE, Tag::Int(i32::from(size)))
    }

    pub fn item_name(self, name: impl Into<String>) -> Self {
        self.put(meta::ITEM_NAME, Tag::String(name.into()))
    }

    pub fn rarity(self, rarity: Rarity) -> Self {
        self.put(meta::RARITY, Tag::from(rarity.key()))
    }

    pub fn enchantment_glint_override(self, glint: bool) -> Self {
        self.put(meta::GLINT, Tag::from(glint))
    }

    pub fn item_model(self, model: impl Into<String>) -> Self {
        self.put(meta::ITEM_MODEL, Tag::String(model.into()))
    }

    pub fn hide_tooltip(self, hide: bool) -> Self {
        self.edit(|components| {
            let (_, hidden) = meta::tooltip_display(components);
            meta::store_tooltip_display(components, hide, &hidden);
        })
    }

    pub fn hide(self, flag: ItemFlag) -> Self {
        self.edit(|components| {
            let (hide, mut hidden) = meta::tooltip_display(components);
            for key in flag.components() {
                if !hidden.iter().any(|known| known == key) {
                    hidden.push((*key).to_owned());
                }
            }
            meta::store_tooltip_display(components, hide, &hidden);
        })
    }

    pub fn show(self, flag: ItemFlag) -> Self {
        self.edit(|components| {
            let (hide, mut hidden) = meta::tooltip_display(components);
            hidden.retain(|known| !flag.components().contains(&known.as_str()));
            meta::store_tooltip_display(components, hide, &hidden);
        })
    }

    pub fn hide_flags<I: IntoIterator<Item = ItemFlag>>(self, flags: I) -> Self {
        flags.into_iter().fold(self, Self::hide)
    }

    pub fn attribute_modifier(self, modifier: AttributeModifier) -> Self {
        self.edit(|components| {
            let mut entries: Vec<Tag> = components
                .get_list(meta::ATTRIBUTE_MODIFIERS)
                .unwrap_or_default()
                .to_vec();
            let replaced = modifier.to_tag();
            let same = |entry: &Tag| {
                AttributeModifier::from_tag(entry).is_some_and(|known| {
                    known.attribute == modifier.attribute && known.id == modifier.id
                })
            };
            match entries.iter_mut().find(|entry| same(entry)) {
                Some(entry) => *entry = replaced,
                None => entries.push(replaced),
            }
            components.insert(meta::ATTRIBUTE_MODIFIERS, Tag::List(entries));
        })
    }

    pub fn attribute_modifiers<I: IntoIterator<Item = AttributeModifier>>(self, all: I) -> Self {
        let tags: Vec<Tag> = all.into_iter().map(|each| each.to_tag()).collect();
        self.put(meta::ATTRIBUTE_MODIFIERS, Tag::List(tags))
    }

    pub fn dyed_color(self, color: Color) -> Self {
        self.put(meta::DYED_COLOR, meta::color_tag(color))
    }

    pub fn potion_contents(self, contents: PotionContents) -> Self {
        self.put(meta::POTION_CONTENTS, contents.to_tag())
    }

    pub fn base_color(self, color: DyeColor) -> Self {
        self.put(meta::BASE_COLOR, meta::dye_tag(color))
    }

    pub fn banner_patterns<I: IntoIterator<Item = BannerLayer>>(self, layers: I) -> Self {
        let tags = layers.into_iter().map(|layer| layer.to_tag()).collect();
        self.put(meta::BANNER_PATTERNS, Tag::List(tags))
    }

    pub fn profile(self, profile: Profile) -> Self {
        self.put(meta::PROFILE, profile.to_tag())
    }

    pub fn written_book_content(self, book: WrittenBook) -> Self {
        self.put(meta::WRITTEN_BOOK, book.to_tag())
    }

    pub fn writable_book_content<I: IntoIterator<Item = S>, S: Into<String>>(
        self,
        pages: I,
    ) -> Self {
        let pages: Vec<String> = pages.into_iter().map(Into::into).collect();
        self.put(meta::WRITABLE_BOOK, meta::writable_tag(&pages))
    }

    pub fn firework_explosion(self, explosion: FireworkExplosion) -> Self {
        self.put(meta::FIREWORK_EXPLOSION, explosion.to_tag())
    }

    pub fn fireworks(self, fireworks: Fireworks) -> Self {
        self.put(meta::FIREWORKS, fireworks.to_tag())
    }

    pub fn trim(self, trim: Trim) -> Self {
        self.put(meta::TRIM, trim.to_tag())
    }

    pub fn bundle_contents(self, stacks: &[ItemStack]) -> Self {
        self.put(meta::BUNDLE_CONTENTS, meta::stack_list(stacks))
    }

    pub fn container(self, stacks: &[ItemStack]) -> Self {
        self.put(meta::CONTAINER, meta::container_tag(stacks))
    }

    pub fn charged_projectiles(self, stacks: &[ItemStack]) -> Self {
        self.put(meta::CHARGED_PROJECTILES, meta::stack_list(stacks))
    }

    pub fn lodestone_tracker(self, tracker: LodestoneTracker) -> Self {
        self.put(meta::LODESTONE_TRACKER, tracker.to_tag())
    }

    pub fn instrument(self, instrument: impl Into<String>) -> Self {
        self.put(meta::INSTRUMENT, Tag::String(instrument.into()))
    }

    pub fn jukebox_playable(self, song: &str) -> Self {
        self.put(meta::JUKEBOX_PLAYABLE, meta::jukebox_tag(song))
    }

    pub fn damage_resistant<I: IntoIterator<Item = S>, S: Into<String>>(self, types: I) -> Self {
        let types: Vec<String> = types.into_iter().map(Into::into).collect();
        self.put(meta::DAMAGE_RESISTANT, meta::resistant_tag(&types))
    }

    pub fn fire_resistant(self, on: bool) -> Self {
        if on {
            self.damage_resistant([meta::FIRE_DAMAGE_TAG])
        } else {
            self.drop_component(meta::DAMAGE_RESISTANT)
        }
    }

    pub fn glider(self, on: bool) -> Self {
        self.switch(meta::GLIDER, on, meta::glider_tag())
    }

    pub fn equippable(self, equippable: Equippable) -> Self {
        self.put(meta::EQUIPPABLE, equippable.to_tag())
    }

    pub fn use_cooldown(self, cooldown: UseCooldown) -> Self {
        self.put(meta::USE_COOLDOWN, cooldown.to_tag())
    }

    pub fn tool(self, tool: Tool) -> Self {
        self.put(meta::TOOL, tool.to_tag())
    }

    pub fn map_id(self, id: i32) -> Self {
        self.put(meta::MAP_ID, Tag::Int(id))
    }

    pub fn meta_set(&mut self, key: &str, value: impl Into<MetaValue>) {
        let tag = value_tag(&value.into());
        let key = key.to_owned();
        let mut components = self.compound();
        data_edit(&mut components, |values| {
            values.insert(key, tag);
        });
        self.components = component::encode(&components);
    }

    pub fn with_meta(mut self, key: &str, value: impl Into<MetaValue>) -> Self {
        self.meta_set(key, value);
        self
    }

    pub fn meta_get(&self, key: &str) -> Option<MetaValue> {
        data_get(&self.compound(), key)
    }

    pub fn meta_has(&self, key: &str) -> bool {
        self.meta_get(key).is_some()
    }

    pub fn meta_keys(&self) -> Vec<String> {
        data_keys(&self.compound())
    }

    pub fn meta_remove(&mut self, key: &str) -> bool {
        let mut components = self.compound();
        let had = data_get(&components, key).is_some();
        if had {
            data_edit(&mut components, |values| {
                values.remove(key);
            });
            self.components = component::encode(&components);
        }
        had
    }

    pub(crate) fn to_nbt(&self) -> Tag {
        let mut components = self.compound();
        if let Some(name) = &self.name {
            components.insert("minecraft:custom_name", name.as_str());
        }
        if !self.lore.is_empty() {
            components.insert(
                "minecraft:lore",
                Tag::List(
                    self.lore
                        .iter()
                        .map(|line| Tag::from(line.as_str()))
                        .collect(),
                ),
            );
        }
        if !self.enchantments.is_empty() {
            let levels: Compound = self
                .enchantments
                .iter()
                .map(|(enchantment, level)| {
                    (
                        enchantment.key().to_owned(),
                        Tag::Int(i32::try_from(*level).unwrap_or(i32::MAX)),
                    )
                })
                .collect();
            components.insert("minecraft:enchantments", levels);
        }
        if let Some(data) = &self.custom_model_data {
            components.insert("minecraft:custom_model_data", custom_model_tag(data));
        }
        let mut out = Compound::new()
            .with("id", self.material.key())
            .with("count", i32::from(self.count));
        if !components.is_empty() {
            out.insert("components", components);
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_nbt(tag: &Tag) -> Option<Self> {
        let stack = tag.as_compound()?;
        let material = Material::from_key(stack.get_str("id")?)?;
        let count = u8::try_from(stack.get_i32("count").unwrap_or(1)).ok()?;
        let mut components = stack
            .get_compound("components")
            .cloned()
            .unwrap_or_default();
        let mut out = Self::new(material, count);
        if let Some(name) = components.remove("minecraft:custom_name") {
            out.name = name.as_str().map(str::to_owned);
        }
        if let Some(Tag::List(lines)) = components.remove("minecraft:lore") {
            out.lore = lines
                .iter()
                .filter_map(|line| line.as_str().map(str::to_owned))
                .collect();
        }
        if let Some(Tag::Compound(found)) = components.remove("minecraft:enchantments") {
            let levels = found.get_compound("levels").unwrap_or(&found);
            out.enchantments = levels
                .iter()
                .filter_map(|(key, level)| {
                    Some((
                        Enchantment::from_key(key)?,
                        u32::try_from(level.as_i64()?).ok()?,
                    ))
                })
                .collect();
        }
        if let Some(data) = components.remove("minecraft:custom_model_data") {
            out.custom_model_data = custom_model_from(&data);
        }
        out.components = component::encode(&components);
        Some(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemMeta {
    components: Compound,
}

impl ItemMeta {
    fn typed<T>(&self, key: &str, read: impl FnOnce(&Tag) -> Option<T>) -> Option<T> {
        self.components.get(key).and_then(read)
    }

    pub fn has(&self, key: &str) -> bool {
        self.components.contains_key(key)
    }

    pub fn component(&self, key: &str) -> Option<ComponentValue> {
        self.components.get(key).map(ComponentValue::from_tag)
    }

    pub fn keys(&self) -> Vec<String> {
        self.components
            .iter()
            .map(|(key, _)| key.to_owned())
            .collect()
    }

    pub fn unbreakable(&self) -> bool {
        self.has(meta::UNBREAKABLE)
    }

    pub fn damage(&self) -> Option<i32> {
        self.components.get_i32(meta::DAMAGE)
    }

    pub fn max_damage(&self) -> Option<i32> {
        self.components.get_i32(meta::MAX_DAMAGE)
    }

    pub fn repair_cost(&self) -> Option<i32> {
        self.components.get_i32(meta::REPAIR_COST)
    }

    pub fn max_stack_size(&self) -> Option<u8> {
        self.components
            .get_i32(meta::MAX_STACK_SIZE)
            .and_then(|size| u8::try_from(size).ok())
    }

    pub fn item_name(&self) -> Option<String> {
        self.components.get_str(meta::ITEM_NAME).map(str::to_owned)
    }

    pub fn rarity(&self) -> Option<Rarity> {
        Rarity::from_key(self.components.get_str(meta::RARITY)?)
    }

    pub fn enchantment_glint_override(&self) -> Option<bool> {
        self.components.get_bool(meta::GLINT)
    }

    pub fn item_model(&self) -> Option<String> {
        self.components.get_str(meta::ITEM_MODEL).map(str::to_owned)
    }

    pub fn hide_tooltip(&self) -> bool {
        meta::tooltip_display(&self.components).0
    }

    pub fn hidden_components(&self) -> Vec<String> {
        meta::tooltip_display(&self.components).1
    }

    pub fn flags(&self) -> Vec<ItemFlag> {
        let hidden = self.hidden_components();
        ItemFlag::ALL
            .iter()
            .copied()
            .filter(|flag| {
                flag.components()
                    .iter()
                    .all(|key| hidden.iter().any(|known| known == key))
            })
            .collect()
    }

    pub fn has_flag(&self, flag: ItemFlag) -> bool {
        self.flags().contains(&flag)
    }

    pub fn attribute_modifiers(&self) -> Vec<AttributeModifier> {
        self.components
            .get_list(meta::ATTRIBUTE_MODIFIERS)
            .unwrap_or_default()
            .iter()
            .filter_map(AttributeModifier::from_tag)
            .collect()
    }

    pub fn dyed_color(&self) -> Option<Color> {
        self.typed(meta::DYED_COLOR, meta::color_from)
    }

    pub fn potion(&self) -> Option<PotionContents> {
        self.typed(meta::POTION_CONTENTS, PotionContents::from_tag)
    }

    pub fn base_color(&self) -> Option<DyeColor> {
        self.typed(meta::BASE_COLOR, meta::dye_from)
    }

    pub fn banner_patterns(&self) -> Vec<BannerLayer> {
        self.components
            .get_list(meta::BANNER_PATTERNS)
            .unwrap_or_default()
            .iter()
            .filter_map(BannerLayer::from_tag)
            .collect()
    }

    pub fn profile(&self) -> Option<Profile> {
        self.typed(meta::PROFILE, Profile::from_tag)
    }

    pub fn written_book(&self) -> Option<WrittenBook> {
        self.typed(meta::WRITTEN_BOOK, WrittenBook::from_tag)
    }

    pub fn writable_book(&self) -> Option<Vec<String>> {
        self.typed(meta::WRITABLE_BOOK, meta::writable_from)
    }

    pub fn firework_explosion(&self) -> Option<FireworkExplosion> {
        self.typed(meta::FIREWORK_EXPLOSION, FireworkExplosion::from_tag)
    }

    pub fn fireworks(&self) -> Option<Fireworks> {
        self.typed(meta::FIREWORKS, Fireworks::from_tag)
    }

    pub fn trim(&self) -> Option<Trim> {
        self.typed(meta::TRIM, Trim::from_tag)
    }

    pub fn bundle_contents(&self) -> Vec<ItemStack> {
        self.components
            .get(meta::BUNDLE_CONTENTS)
            .map(meta::stack_list_from)
            .unwrap_or_default()
    }

    pub fn container(&self) -> Vec<ItemStack> {
        self.components
            .get(meta::CONTAINER)
            .map(meta::container_from)
            .unwrap_or_default()
    }

    pub fn charged_projectiles(&self) -> Vec<ItemStack> {
        self.components
            .get(meta::CHARGED_PROJECTILES)
            .map(meta::stack_list_from)
            .unwrap_or_default()
    }

    pub fn lodestone_tracker(&self) -> Option<LodestoneTracker> {
        self.typed(meta::LODESTONE_TRACKER, LodestoneTracker::from_tag)
    }

    pub fn instrument(&self) -> Option<String> {
        self.components.get_str(meta::INSTRUMENT).map(str::to_owned)
    }

    pub fn jukebox_playable(&self) -> Option<String> {
        self.typed(meta::JUKEBOX_PLAYABLE, meta::jukebox_from)
    }

    pub fn damage_resistant(&self) -> Option<Vec<String>> {
        self.typed(meta::DAMAGE_RESISTANT, meta::resistant_from)
    }

    pub fn fire_resistant(&self) -> bool {
        self.damage_resistant()
            .is_some_and(|types| types.iter().any(|name| name == meta::FIRE_DAMAGE_TAG))
    }

    pub fn glider(&self) -> bool {
        self.has(meta::GLIDER)
    }

    pub fn equippable(&self) -> Option<Equippable> {
        self.typed(meta::EQUIPPABLE, Equippable::from_tag)
    }

    pub fn use_cooldown(&self) -> Option<UseCooldown> {
        self.typed(meta::USE_COOLDOWN, UseCooldown::from_tag)
    }

    pub fn tool(&self) -> Option<Tool> {
        self.typed(meta::TOOL, Tool::from_tag)
    }

    pub fn map_id(&self) -> Option<i32> {
        self.components.get_i32(meta::MAP_ID)
    }

    pub fn data_get(&self, key: &str) -> Option<MetaValue> {
        data_get(&self.components, key)
    }

    pub fn data_keys(&self) -> Vec<String> {
        data_keys(&self.components)
    }
}

#[cfg(test)]
mod tests {
    use rspigot_plugin_protocol::v1;

    use super::*;

    fn network(components: &Compound) -> Vec<u8> {
        component::encode(components)
    }

    fn wire(components: Vec<u8>) -> ItemStack {
        ItemStack::from_wire(v1::Item {
            key: "minecraft:stick".to_owned(),
            count: 1,
            components,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn an_unknown_component_survives_typed_edits_byte_for_byte() {
        let odd = Compound::new()
            .with("zeta", 1_i16)
            .with("alpha", Tag::List(vec![Tag::Float(1.5), Tag::Float(2.5)]))
            .with("mid", Tag::IntArray(vec![9, 8, 7]));
        let bytes = network(&Compound::new().with("mod:unknown", odd.clone()));
        let item = wire(bytes.clone());
        assert_eq!(item.to_wire().components, bytes);
        let edited = item.clone().unbreakable(true).unbreakable(false);
        assert_eq!(edited.to_wire().components, bytes);
        let with_data = item.with_meta("a:b", 1);
        let mut back = with_data.clone();
        back.meta_remove("a:b");
        assert_eq!(back.to_wire().components, bytes);
        assert_eq!(
            with_data.compound().get("mod:unknown"),
            Some(&Tag::Compound(odd))
        );
    }

    #[test]
    fn foreign_public_values_stay_untouched_and_readable() {
        let foreign = Compound::new()
            .with("spigot:level", 5_i32)
            .with("spigot:weird", Tag::List(vec![Tag::Int(1)]))
            .with("spigot:name", "x");
        let custom = Compound::new()
            .with(PUBLIC_VALUES, foreign.clone())
            .with("other", 1_i32);
        let mut item = wire(network(&Compound::new().with(CUSTOM_DATA, custom)));
        assert_eq!(item.meta_get("spigot:level"), Some(MetaValue::Int(5)));
        assert_eq!(
            item.meta_get("spigot:name"),
            Some(MetaValue::String("x".to_owned()))
        );
        assert_eq!(item.meta_get("spigot:weird"), None);
        assert_eq!(item.meta_keys(), ["spigot:level", "spigot:name"]);
        item.meta_set("relic:kind", "fire");
        let stored = item.compound();
        let data = stored.get_compound(CUSTOM_DATA).unwrap();
        assert_eq!(data.get_i32("other"), Some(1));
        let values = data.get_compound(PUBLIC_VALUES).unwrap();
        assert_eq!(values.get("spigot:weird"), foreign.get("spigot:weird"));
        assert_eq!(values.get("spigot:level"), foreign.get("spigot:level"));
        assert!(item.meta_remove("relic:kind"));
        assert_eq!(
            item.compound()
                .get_compound(CUSTOM_DATA)
                .unwrap()
                .get_compound(PUBLIC_VALUES),
            Some(&foreign)
        );
    }

    #[test]
    fn a_nested_stack_round_trips_its_typed_fields() {
        let inner = ItemStack::new(Material::Stick, 2)
            .name("n")
            .lore(["l"])
            .custom_model_data(CustomModelData {
                floats: vec![1.0],
                ..CustomModelData::default()
            })
            .enchant(Enchantment::Mending, 1)
            .with_meta("a:b", true);
        assert_eq!(ItemStack::from_nbt(&inner.to_nbt()), Some(inner));
    }
}
