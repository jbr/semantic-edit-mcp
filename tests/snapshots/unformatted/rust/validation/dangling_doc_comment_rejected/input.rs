struct Counter {
    count: u32,
}

impl Counter {
    fn increment(&mut self) {
        self.count += 1;
    }
}
