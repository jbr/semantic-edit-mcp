pub struct Shelf {
    books: Vec<String>,
}

impl Shelf {
    pub fn new() -> Self {
        Self { books: Vec::new() }
    }

    pub fn add(&mut self, book: String) {
        let count = self.books.len();
        self.books.push(book);
    }

    pub fn len(&self) -> usize {
        self.books.len()
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }
}
