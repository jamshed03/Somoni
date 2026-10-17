use std::collections::HashMap;

/// The open blocks and the names declared in each of them.
/// `T` is what the type checker knows about a name.
pub struct Scopes<T> {
    stack: Vec<HashMap<String, T>>,
}

impl<T> Scopes<T> {
    pub fn new() -> Scopes<T> {
        Scopes {
            stack: vec![HashMap::new()],
        }
    }

    /// Opens a new block.
    pub fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    /// Closes the innermost block; its names are gone.
    pub fn pop(&mut self) {
        self.stack.pop();
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
