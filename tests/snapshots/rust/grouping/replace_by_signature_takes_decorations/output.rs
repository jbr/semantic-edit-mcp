pub struct Counter {
    count: u32,
}

impl Counter {
    /// Creates a counter at zero.
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// Adds `by` to the count, stopping at the maximum.
    #[inline]
    pub fn increment(&mut self, by: u32) {
        self.count = self.count.saturating_add(by);
    }

    /// The current count.
    pub fn get(&self) -> u32 {
        self.count
    }
}
