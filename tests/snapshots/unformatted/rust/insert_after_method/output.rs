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
    pub fn increment(&mut self) {
        self.count += 1;
    }

    /// The current count.
    pub fn get(&self) -> u32 {
        self.count
    }
    /// Whether nothing has been counted.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}
