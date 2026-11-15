use crate::{Enchantment, Material};

#[derive(Debug, Clone, Default)]
pub struct CustomModelData {
    pub floats: Vec<f32>,
    pub flags: Vec<bool>,
    pub strings: Vec<String>,
    pub colors: Vec<i32>,
}

impl PartialEq for CustomModelData {
    fn eq(&self, other: &Self) -> bool {
        self.floats.len() == other.floats.len()
            && self
                .floats
                .iter()
                .zip(&other.floats)
                .all(|(a, b)| a.to_bits() == b.to_bits())
            && self.flags == other.flags
            && self.strings == other.strings
            && self.colors == other.colors
    }
}

impl Eq for CustomModelData {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStack {
    pub material: Material,
    pub count: u8,
    pub name: Option<String>,
    pub lore: Vec<String>,
    pub enchantments: Vec<(Enchantment, u32)>,
    pub custom_model_data: Option<CustomModelData>,
    pub(crate) components: Vec<u8>,
}

impl ItemStack {
    pub const fn new(material: Material, count: u8) -> Self {
        Self {
            material,
            count,
            name: None,
            lore: Vec::new(),
            enchantments: Vec::new(),
            custom_model_data: None,
            components: Vec::new(),
        }
    }

    pub fn name(self, text: impl Into<String>) -> Self {
        Self {
            name: Some(text.into()),
            ..self
        }
    }

    pub fn lore<I: IntoIterator<Item = S>, S: Into<String>>(self, lines: I) -> Self {
        Self {
            lore: lines.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    pub fn custom_model_data(self, data: CustomModelData) -> Self {
        Self {
            custom_model_data: Some(data),
            ..self
        }
    }

    pub fn enchant(mut self, enchantment: Enchantment, level: u32) -> Self {
        match self
            .enchantments
            .iter_mut()
            .find(|(known, _)| *known == enchantment)
        {
            Some((_, known)) => *known = level,
            None => self.enchantments.push((enchantment, level)),
        }
        self
    }
}
