use std::collections::HashMap;

// Stack of open blocks and the names declared in them.
// T is what a name stands for: VarInfo (type, const) in the type checker, Value in the interpreter.
pub struct Scopes<T> {
    stack: Vec<HashMap<String, T>>,
}

impl<T> Scopes<T> {
    pub fn new() -> Scopes<T> {
        Scopes {
            stack: vec![HashMap::new()],
        }
    }

    pub fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn declare(&mut self, name: &str, value: T) -> bool {
        let scope = self.stack.last_mut().expect("scope stack is never empty");
        if scope.contains_key(name) {
            return false;
        }
        scope.insert(name.to_string(), value);
        true
    }

    pub fn lookup(&self, name: &str) -> Option<&T> {
        self.stack.iter().rev().find_map(|scope| scope.get(name))
    }

    pub fn lookup_mut(&mut self, name: &str) -> Option<&mut T> {
        self.stack
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
    }
}
