// Copyright (c) 2024 webyep
// Licensed under the MIT License.

use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: HashMap<String, Node>,
    pub modified: std::time::SystemTime,
    pub accessed: std::time::SystemTime,
}

impl Default for Node {
    fn default() -> Self {
        Self::new(String::new(), PathBuf::new(), false)
    }
}

impl Node {
    pub fn new(name: String, path: PathBuf, is_dir: bool) -> Self {
        Self {
            name,
            path,
            size: 0,
            is_dir,
            children: HashMap::new(),
            modified: std::time::SystemTime::UNIX_EPOCH,
            accessed: std::time::SystemTime::UNIX_EPOCH,
        }
    }

    pub fn insert_child(&mut self, node: Node) {
        self.children.insert(node.name.clone(), node);
    }

    pub fn update_size(&mut self) -> u64 {
        if !self.is_dir {
            return self.size;
        }
        self.size = self.children.values_mut().map(|c| c.update_size()).sum();
        self.size
    }
}
