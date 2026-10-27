pub struct Counter {
    count: u32,
}

impl Counter {
    /// Creates a counter at zero.
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// Adds one to the count.
    #[inline]
    pub fn increment(&mut self, by: u32) {
        self.count += 1;
    }

    /// The current count.
    pub fn get(&self) -> u32 {
        self.count
    }
}
