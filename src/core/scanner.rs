// Copyright (c) 2024 webyep
// Licensed under the MIT License.

use crate::core::tree::Node;
use std::path::PathBuf;
use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use rayon::prelude::*;

pub struct Scanner;

impl Scanner {
    pub fn scan_incremental(
        path: PathBuf,
        root: Arc<RwLock<Node>>,
        is_scanning: Arc<AtomicBool>,
        items_scanned: Arc<AtomicU64>,
        skipped_files: Arc<AtomicU64>,
    ) -> Result<()> {
        let scanned_node = Self::scan_dir(path, &items_scanned, &skipped_files);
        {
            let mut root_locked = root.write().unwrap();
            *root_locked = scanned_node;
        }
        is_scanning.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn scan_dir(path: PathBuf, items_scanned: &AtomicU64, skipped_files: &AtomicU64) -> Node {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        
        let mut node = Node::new(name, path.clone(), true);
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(m) = meta.modified() {
                node.modified = m;
            }
            if let Ok(a) = meta.accessed() {
                node.accessed = a;
            }
        }

        let entries = match std::fs::read_dir(&path) {
            Ok(read_dir) => {
                let mut list = Vec::new();
                for entry in read_dir {
                    match entry {
                        Ok(e) => list.push(e),
                        Err(_) => {
                            skipped_files.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                list
            }
            Err(_) => {
                skipped_files.fetch_add(1, Ordering::Relaxed);
                return node;
            }
        };

        let mut subdirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries {
            if let Ok(ft) = entry.file_type() {
                if ft.is_symlink() {
                    skipped_files.fetch_add(1, Ordering::Relaxed);
                    continue; // Skip symlinks
                }
                let metadata = entry.metadata().ok();
                let modified = metadata.as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let accessed = metadata.as_ref()
                    .and_then(|m| m.accessed().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                if ft.is_dir() {
                    subdirs.push((entry.path(), modified, accessed));
                } else {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let len = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    files.push((name, entry.path(), len, modified, accessed));
                }
            } else {
                skipped_files.fetch_add(1, Ordering::Relaxed);
            }
        }

        items_scanned.fetch_add((subdirs.len() + files.len()) as u64, Ordering::Relaxed);

        let subdir_nodes: Vec<Node> = if subdirs.is_empty() {
            Vec::new()
        } else {
            subdirs.into_par_iter()
                .map(|(subpath, _, _)| Self::scan_dir(subpath, items_scanned, skipped_files))
                .collect()
        };

        for snode in subdir_nodes {
            node.insert_child(snode);
        }

        for (fname, fpath, flen, fmod, facc) in files {
            let mut fnode = Node::new(fname, fpath, false);
            fnode.size = flen;
            fnode.modified = fmod;
            fnode.accessed = facc;
            node.insert_child(fnode);
        }

        node.update_size();
        node
    }
}
