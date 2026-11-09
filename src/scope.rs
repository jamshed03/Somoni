use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The names declared in one block, hashed with `FastHasher`.
type Block<T> = HashMap<String, T, BuildHasherDefault<FastHasher>>;

/// The open blocks and what is known about each name declared in them.
pub struct Scopes<T> {
    stack: Vec<Block<T>>,
}

impl<T> Scopes<T> {
    /// Starts with one open block that is never closed.
    pub fn new() -> Scopes<T> {
        Scopes {
            stack: vec![Block::default()],
        }
    }

    /// Opens a new block.
    pub fn push(&mut self) {
        self.stack.push(Block::default());
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

    /// Adds a name to the innermost block, or replaces what the block knows about it.
    pub fn set_innermost(&mut self, name: &str, value: T) {
        if let Some(scope) = self.stack.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    /// Lets `change` change what every open block knows about `name`.
    pub fn update_all(&mut self, name: &str, mut change: impl FnMut(&mut T)) {
        for scope in &mut self.stack {
            if let Some(value) = scope.get_mut(name) {
                change(value);
            }
        }
    }

    /// Finds a name, starting with the innermost block.
    pub fn lookup(&self, name: &str) -> Option<&T> {
        self.stack.iter().rev().find_map(|scope| scope.get(name))
    }
}

/// A fast hasher for names from the source code, like the one inside the Rust compiler (FxHash).
/// Unlike the default SipHash it gives no protection against keys chosen to collide, so it is only for names the
/// programmer wrote, never for data from outside like form fields.
#[derive(Default)]
struct FastHasher {
    hash: u64,
}

impl FastHasher {
    /// Mixes one word into the hash.
    fn add(&mut self, word: u64) {
        const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    /// Mixes the bytes in eight at a time, then the rest one by one.
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        for &byte in rest {
            self.add(u64::from(byte));
        }
    }

    /// Returns the hash.
    fn finish(&self) -> u64 {
        self.hash
    }
}
