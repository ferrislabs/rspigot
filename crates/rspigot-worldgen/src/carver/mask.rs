#[derive(Debug, Clone)]
pub(crate) struct Mask {
    min_y: i32,
    max_y: i32,
    height: usize,
    bits: Vec<bool>,
    carved: usize,
}

impl Mask {
    pub(crate) fn new(min_y: i32, max_y: i32) -> Self {
        let height = (max_y - min_y + 1).max(0) as usize;
        Self {
            min_y,
            max_y,
            height,
            bits: vec![false; height * 256],
            carved: 0,
        }
    }

    pub(crate) fn min_y(&self) -> i32 {
        self.min_y
    }

    pub(crate) fn max_y(&self) -> i32 {
        self.max_y
    }

    fn index(&self, x: usize, y: i32, z: usize) -> usize {
        (y - self.min_y) as usize + (z + x * 16) * self.height
    }

    pub(crate) fn carve(&mut self, x: usize, y: i32, z: usize) {
        let index = self.index(x, y, z);
        if !self.bits[index] {
            self.bits[index] = true;
            self.carved += 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, x: usize, y: i32, z: usize) -> bool {
        y >= self.min_y && y <= self.max_y && self.bits[self.index(x, y, z)]
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.carved == 0
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.carved
    }

    pub(crate) fn segments(&self, x: usize, z: usize) -> Vec<(i32, i32)> {
        let mut found = Vec::new();
        let mut start = None;
        for y in self.min_y..=self.max_y {
            let set = self.bits[self.index(x, y, z)];
            match (set, start) {
                (true, None) => start = Some(y),
                (false, Some(from)) => {
                    found.push((from, y - 1));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(from) = start {
            found.push((from, self.max_y));
        }
        found
    }
}
