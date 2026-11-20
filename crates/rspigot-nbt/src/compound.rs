use indexmap::IndexMap;

use crate::tag::Tag;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound(IndexMap<String, Tag>);

impl Compound {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(IndexMap::with_capacity(capacity))
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Tag>) -> Option<Tag> {
        self.0.insert(key.into(), value.into())
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<Tag>) -> Self {
        self.insert(key, value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
        self.0.get_mut(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<Tag> {
        self.0.shift_remove(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Tag)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Tag> {
        self.0.values_mut()
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_i64()
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get_i64(key).and_then(|v| i32::try_from(v).ok())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.as_f64()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get_i64(key).map(|v| v != 0)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn get_compound(&self, key: &str) -> Option<&Compound> {
        self.get(key)?.as_compound()
    }

    pub fn get_list(&self, key: &str) -> Option<&[Tag]> {
        self.get(key)?.as_list()
    }

    pub fn get_long_array(&self, key: &str) -> Option<&[i64]> {
        match self.get(key)? {
            Tag::LongArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_int_array(&self, key: &str) -> Option<&[i32]> {
        match self.get(key)? {
            Tag::IntArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_byte_array(&self, key: &str) -> Option<&[u8]> {
        match self.get(key)? {
            Tag::ByteArray(v) => Some(v),
            _ => None,
        }
    }
}

impl FromIterator<(String, Tag)> for Compound {
    fn from_iter<I: IntoIterator<Item = (String, Tag)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Compound {
    type Item = (String, Tag);
    type IntoIter = indexmap::map::IntoIter<String, Tag>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
