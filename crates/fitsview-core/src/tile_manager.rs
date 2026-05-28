use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct TileKey {
    pub file_id: u64,
    pub hdu: usize,
    pub tile_size: usize,
    pub tx: usize,
    pub ty: usize,
}

pub struct TileData {
    pub pixels: Vec<f32>,
    pub width: usize,
    pub height: usize,
}

pub struct TileManager {
    cache: LruCache<TileKey, Arc<TileData>>,
    current_bytes: usize,
    max_bytes: usize,
    next_file_id: u64,
}

impl TileManager {
    pub const TILE_SIZE: usize = 512;
    pub const MAX_MEMORY: usize = 512 * 1024 * 1024;

    pub fn new(max_bytes: usize) -> Self {
        // Set a large cap so the LruCache never evicts on its own;
        // eviction is managed entirely by evict_to_budget() via byte accounting.
        // max_bytes / 4 gives an upper bound on f32-pixel tiles of any size.
        let cap = (max_bytes / 4).max(1);
        Self {
            cache: LruCache::new(NonZeroUsize::new(cap).unwrap()),
            current_bytes: 0,
            max_bytes,
            next_file_id: 1,
        }
    }

    /// Assign a new unique file_id. Call once per opened large file.
    pub fn register_file(&mut self) -> u64 {
        let id = self.next_file_id;
        self.next_file_id += 1;
        id
    }

    pub fn get(&mut self, key: &TileKey) -> Option<Arc<TileData>> {
        self.cache.get(key).cloned()
    }

    pub fn insert(&mut self, key: TileKey, tile: TileData) {
        let bytes = tile.pixels.len() * 4;
        // If the same key is already cached, subtract its old size first.
        if let Some(old) = self.cache.peek(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(old.pixels.len() * 4);
        }
        self.cache.put(key, Arc::new(tile));
        self.current_bytes += bytes;
        self.evict_to_budget();
    }

    pub fn memory_used(&self) -> usize {
        self.current_bytes
    }

    fn evict_to_budget(&mut self) {
        while self.current_bytes > self.max_bytes {
            match self.cache.pop_lru() {
                Some((_, evicted)) => {
                    self.current_bytes =
                        self.current_bytes.saturating_sub(evicted.pixels.len() * 4);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tile(w: usize, h: usize) -> TileData {
        TileData { pixels: vec![0.0f32; w * h], width: w, height: h }
    }

    fn key(file_id: u64, tx: usize, ty: usize) -> TileKey {
        TileKey { file_id, hdu: 0, tile_size: 512, tx, ty }
    }

    #[test]
    fn test_insert_and_get() {
        let mut mgr = TileManager::new(TileManager::MAX_MEMORY);
        mgr.insert(key(1, 0, 0), make_tile(512, 512));
        assert!(mgr.get(&key(1, 0, 0)).is_some());
    }

    #[test]
    fn test_lru_eviction() {
        // Budget for exactly 2 full tiles (512×512×4 = 1MB each)
        let tile_bytes = 512 * 512 * 4;
        let mut mgr = TileManager::new(tile_bytes * 2);
        mgr.insert(key(1, 0, 0), make_tile(512, 512));
        mgr.insert(key(1, 1, 0), make_tile(512, 512));
        // Inserting a 3rd tile should evict the LRU (0,0)
        mgr.insert(key(1, 2, 0), make_tile(512, 512));
        assert!(mgr.get(&key(1, 0, 0)).is_none(), "LRU tile should be evicted");
        assert!(mgr.get(&key(1, 1, 0)).is_some());
        assert!(mgr.get(&key(1, 2, 0)).is_some());
        assert!(mgr.memory_used() <= tile_bytes * 2);
    }
}
