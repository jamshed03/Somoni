use std::collections::HashMap;

/// The open blocks and what is known about each name declared in them.
pub struct Scopes<T> {
    stack: Vec<HashMap<String, T>>,
}

impl<T> Scopes<T> {
    /// Starts with one open block that is never closed.
    pub fn new() -> Scopes<T> {
        Scopes {
            stack: vec![HashMap::new()],
        }
    }

    /// Opens a new block.
    pub fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    /// Closes the innermost block and forgets its names, but never the first block.
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Adds a name to the innermost block. Returns false if the block already has it.
    pub fn declare(&mut self, name: &str, value: T) -> bool {
        let scope = self.stack.last_mut().expect("scope stack is never empty");
        if scope.contains_key(name) {
            return false;
        }
        scope.insert(name.to_string(), value);
        true
    }

    /// Finds a name, starting with the innermost block.
    pub fn lookup(&self, name: &str) -> Option<&T> {
        self.stack.iter().rev().find_map(|scope| scope.get(name))
    }
}
