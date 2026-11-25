pub const LIGHT_ARRAY_LEN: usize = 2048;

const fn build_uniform() -> [[u8; LIGHT_ARRAY_LEN]; 16] {
    let mut table = [[0u8; LIGHT_ARRAY_LEN]; 16];
    let mut n = 0;
    while n < 16 {
        let mut i = 0;
        while i < LIGHT_ARRAY_LEN {
            table[n][i] = (n as u8) | ((n as u8) << 4);
            i += 1;
        }
        n += 1;
    }
    table
}

static UNIFORM: [[u8; LIGHT_ARRAY_LEN]; 16] = build_uniform();

#[derive(Clone)]
enum Repr {
    Uniform(u8),
    Dense(Box<[u8; LIGHT_ARRAY_LEN]>),
}

#[derive(Clone)]
pub struct LightArray(Repr);

fn uniform_nibble(bytes: &[u8; LIGHT_ARRAY_LEN]) -> Option<u8> {
    let first = bytes[0];
    if first & 0x0F == first >> 4 && bytes.iter().all(|b| *b == first) {
        Some(first & 0x0F)
    } else {
        None
    }
}

impl LightArray {
    pub fn filled(level: u8) -> Self {
        Self(Repr::Uniform(level & 0x0F))
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let array: &[u8; LIGHT_ARRAY_LEN] = bytes.try_into().ok()?;
        Some(match uniform_nibble(array) {
            Some(n) => Self(Repr::Uniform(n)),
            None => Self(Repr::Dense(Box::new(*array))),
        })
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        match &self.0 {
            Repr::Uniform(n) => *n,
            Repr::Dense(bytes) => {
                let index = (y << 8) | (z << 4) | x;
                (bytes[index >> 1] >> ((index & 1) * 4)) & 0x0F
            }
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, level: u8) {
        let level = level & 0x0F;
        if let Repr::Uniform(n) = self.0 {
            if n == level {
                return;
            }
            self.0 = Repr::Dense(Box::new([n | (n << 4); LIGHT_ARRAY_LEN]));
        }
        if let Repr::Dense(bytes) = &mut self.0 {
            let index = (y << 8) | (z << 4) | x;
            let shift = (index & 1) * 4;
            let byte = &mut bytes[index >> 1];
            *byte = (*byte & !(0x0F << shift)) | (level << shift);
        }
    }

    pub fn as_bytes(&self) -> &[u8; LIGHT_ARRAY_LEN] {
        match &self.0 {
            Repr::Uniform(n) => &UNIFORM[*n as usize],
            Repr::Dense(bytes) => bytes,
        }
    }

    pub fn is_uniform(&self, level: u8) -> bool {
        let nibble = level & 0x0F;
        match &self.0 {
            Repr::Uniform(n) => *n == nibble,
            Repr::Dense(bytes) => {
                let byte = nibble | (nibble << 4);
                bytes.iter().all(|b| *b == byte)
            }
        }
    }

    pub fn compact(&mut self) {
        if let Repr::Dense(bytes) = &self.0
            && let Some(n) = uniform_nibble(bytes)
        {
            self.0 = Repr::Uniform(n);
        }
    }

    pub fn is_dense(&self) -> bool {
        matches!(self.0, Repr::Dense(_))
    }
}

impl PartialEq for LightArray {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Uniform(a), Repr::Uniform(b)) => a == b,
            _ => self.as_bytes() == other.as_bytes(),
        }
    }
}

impl Eq for LightArray {}

impl std::fmt::Debug for LightArray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Repr::Uniform(n) => write!(f, "LightArray::Uniform({n})"),
            Repr::Dense(_) => write!(f, "LightArray::Dense(..)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkLight {
    pub sky: Vec<Option<LightArray>>,
    pub block: Vec<Option<LightArray>>,
}

impl ChunkLight {
    pub fn new(section_count: usize) -> Self {
        Self {
            sky: vec![None; section_count + 2],
            block: vec![None; section_count + 2],
        }
    }

    pub fn light_section_count(&self) -> usize {
        self.sky.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filled_array_has_no_buffer() {
        let a = LightArray::filled(15);
        assert!(!a.is_dense());
        assert_eq!(a.as_bytes(), &[0xFF; LIGHT_ARRAY_LEN]);
        assert!(a.is_uniform(15));
        assert!(!a.is_uniform(0));
    }

    #[test]
    fn a_differing_write_promotes_to_dense_and_keeps_reads() {
        let mut a = LightArray::filled(15);
        a.set(3, 4, 5, 15);
        assert!(!a.is_dense());
        a.set(3, 4, 5, 7);
        assert_eq!(a.get(3, 4, 5), 7);
        assert_eq!(a.get(0, 0, 0), 15);
        assert_eq!(a.get(4, 4, 5), 15);
        assert!(a.is_dense());
    }

    #[test]
    fn from_bytes_on_identical_bytes_yields_uniform_equal_to_dense() {
        let u = LightArray::from_bytes(&[0x77; LIGHT_ARRAY_LEN]).unwrap();
        assert!(!u.is_dense());
        assert_eq!(u, LightArray::filled(7));
        let mut d = LightArray::filled(7);
        d.set(1, 2, 3, 2);
        d.set(1, 2, 3, 7);
        assert!(d.is_dense());
        assert_eq!(d, LightArray::filled(7));
        assert_eq!(LightArray::filled(7), d);
        let m = LightArray::from_bytes(&[0x7A; LIGHT_ARRAY_LEN]).unwrap();
        assert!(m.is_dense());
    }

    #[test]
    fn compact_demotes_a_dense_array_that_became_uniform() {
        let mut a = LightArray::filled(0);
        a.set(1, 1, 1, 5);
        a.set(1, 1, 1, 0);
        assert!(a.is_dense());
        a.compact();
        assert!(!a.is_dense());
        assert_eq!(a.as_bytes(), &[0; LIGHT_ARRAY_LEN]);
        let mut b = LightArray::filled(0);
        b.set(1, 1, 1, 5);
        b.compact();
        assert!(b.is_dense());
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert!(LightArray::from_bytes(&[0; 10]).is_none());
        assert!(LightArray::from_bytes(&[0; LIGHT_ARRAY_LEN + 1]).is_none());
    }

    #[test]
    fn an_optional_array_stays_small() {
        assert!(std::mem::size_of::<Option<LightArray>>() <= 16);
    }

    #[test]
    fn nibble_roundtrip() {
        let mut a = LightArray::filled(0);
        a.set(3, 4, 5, 12);
        a.set(4, 4, 5, 7);
        assert_eq!(a.get(3, 4, 5), 12);
        assert_eq!(a.get(4, 4, 5), 7);
        assert_eq!(a.get(5, 4, 5), 0);
    }
}
