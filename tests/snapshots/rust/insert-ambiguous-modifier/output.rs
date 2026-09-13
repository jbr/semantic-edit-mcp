use std::collections::HashMap;

pub struct UserStore {
    users: HashMap<u64, String>,
}

impl UserStore {
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    pub fn add(&mut self, id: u64, name: String) {
        self.users.insert(id, name);
    }
}
