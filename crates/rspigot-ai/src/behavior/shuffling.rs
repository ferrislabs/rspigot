use crate::random::RandomSource;

#[derive(Debug, Clone, PartialEq)]
struct Weighted<T> {
    data: T,
    weight: i32,
    rand_weight: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShufflingList<T> {
    entries: Vec<Weighted<T>>,
}

impl<T> Default for ShufflingList<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> ShufflingList<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, data: T, weight: i32) {
        self.entries.push(Weighted {
            data,
            weight,
            rand_weight: 0.0,
        });
    }

    pub fn shuffle(&mut self, random: &mut impl RandomSource) {
        for entry in &mut self.entries {
            let draw = f64::from(random.next_float());
            let exponent = f64::from(1.0_f32 / entry.weight as f32);
            entry.rand_weight = -draw.powf(exponent);
        }
        self.entries
            .sort_by(|a, b| a.rand_weight.total_cmp(&b.rand_weight));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.entries.get(index).map(|entry| &entry.data)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.entries.get_mut(index).map(|entry| &mut entry.data)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|entry| &entry.data)
    }

    pub fn weight(&self, index: usize) -> Option<i32> {
        self.entries.get(index).map(|entry| entry.weight)
    }
}

impl<T> FromIterator<(T, i32)> for ShufflingList<T> {
    fn from_iter<I: IntoIterator<Item = (T, i32)>>(iter: I) -> Self {
        let mut list = Self::new();
        for (data, weight) in iter {
            list.add(data, weight);
        }
        list
    }
}
