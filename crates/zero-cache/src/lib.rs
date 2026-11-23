#![forbid(unsafe_code)]

use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub current_items: usize,
    pub current_bytes: usize,
    pub max_bytes: usize,
}

impl CacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

pub trait ByteSized {
    fn byte_size(&self) -> usize;
}

impl ByteSized for Arc<[u8]> {
    fn byte_size(&self) -> usize {
        self.len()
    }
}

impl ByteSized for String {
    fn byte_size(&self) -> usize {
        self.capacity()
    }
}

struct CacheEntry<V> {
    value: V,
    bytes: usize,
}

/// A bounded LRU cache enforcing both maximum item count and maximum byte budget.
pub struct BoundedCache<K: Hash + Eq, V> {
    lru: LruCache<K, CacheEntry<V>>,
    max_bytes: usize,
    current_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl<K: Hash + Eq, V: ByteSized> BoundedCache<K, V> {
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        let cap = NonZeroUsize::new(max_items.max(1)).expect("non-zero capacity");
        Self {
            lru: LruCache::new(cap),
            max_bytes,
            current_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(entry) = self.lru.get(key) {
            self.hits += 1;
            Some(&entry.value)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        let entry_bytes = value.byte_size();
        if entry_bytes > self.max_bytes {
            return;
        }

        if let Some(old) = self.lru.pop(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(old.bytes);
        }

        while self.lru.len() >= self.lru.cap().get()
            || self.current_bytes + entry_bytes > self.max_bytes
        {
            if let Some((_, evicted)) = self.lru.pop_lru() {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.bytes);
                self.evictions += 1;
            } else {
                break;
            }
        }

        self.current_bytes += entry_bytes;
        self.lru.put(
            key,
            CacheEntry {
                value,
                bytes: entry_bytes,
            },
        );
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.lru.pop(key).map(|entry| {
            self.current_bytes = self.current_bytes.saturating_sub(entry.bytes);
            entry.value
        })
    }

    pub fn clear(&mut self) {
        self.lru.clear();
        self.current_bytes = 0;
    }

    pub fn metrics(&self) -> CacheMetrics {
        CacheMetrics {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            current_items: self.lru.len(),
            current_bytes: self.current_bytes,
            max_bytes: self.max_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LazyImageState {
    Unloaded,
    Decoding,
    Ready {
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
    },
    Failed(String),
}

impl ByteSized for LazyImageState {
    fn byte_size(&self) -> usize {
        match self {
            Self::Ready { rgba, .. } => rgba.len(),
            Self::Failed(msg) => msg.len(),
            Self::Unloaded | Self::Decoding => 0,
        }
    }
}

/// Bounded lazy image cache for avatars, emojis, and message attachments.
pub struct ImageCache {
    inner: BoundedCache<String, LazyImageState>,
}

impl ImageCache {
    pub fn new(max_images: usize, max_bytes: usize) -> Self {
        Self {
            inner: BoundedCache::new(max_images, max_bytes),
        }
    }

    pub fn state(&mut self, key: &str) -> LazyImageState {
        self.inner
            .get(&key.to_string())
            .cloned()
            .unwrap_or(LazyImageState::Unloaded)
    }

    pub fn mark_decoding(&mut self, key: impl Into<String>) {
        self.inner.insert(key.into(), LazyImageState::Decoding);
    }

    pub fn store_decoded(
        &mut self,
        key: impl Into<String>,
        width: u32,
        height: u32,
        rgba: Arc<[u8]>,
    ) {
        self.inner.insert(
            key.into(),
            LazyImageState::Ready {
                width,
                height,
                rgba,
            },
        );
    }

    pub fn mark_failed(&mut self, key: impl Into<String>, err: impl Into<String>) {
        self.inner
            .insert(key.into(), LazyImageState::Failed(err.into()));
    }

    pub fn metrics(&self) -> CacheMetrics {
        self.inner.metrics()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_cache_enforces_byte_budget_and_tracks_metrics() {
        let mut cache: BoundedCache<u32, Arc<[u8]>> = BoundedCache::new(10, 100);
        cache.insert(1, Arc::from(vec![0u8; 40]));
        cache.insert(2, Arc::from(vec![1u8; 40]));
        assert_eq!(cache.metrics().current_bytes, 80);

        // Accessing key 1 makes key 2 the LRU item.
        assert!(cache.get(&1).is_some());
        assert!(cache.get(&999).is_none());

        // Inserting 40 bytes exceeds 100-byte budget, so key 2 must be evicted.
        cache.insert(3, Arc::from(vec![2u8; 40]));
        let m = cache.metrics();
        assert_eq!(m.current_bytes, 80);
        assert_eq!(m.evictions, 1);
        assert_eq!(m.hits, 1);
        assert_eq!(m.misses, 1);
        assert!(cache.get(&2).is_none());
        assert!(cache.get(&1).is_some());
    }

    #[test]
    fn oversize_entry_is_rejected_without_evicting_existing() {
        let mut cache: BoundedCache<u32, Arc<[u8]>> = BoundedCache::new(4, 64);
        cache.insert(1, Arc::from(vec![0u8; 32]));
        cache.insert(2, Arc::from(vec![1u8; 128]));
        assert_eq!(cache.metrics().current_items, 1);
        assert_eq!(cache.metrics().current_bytes, 32);
    }

    #[test]
    fn lazy_image_cache_transitions_and_evicts() {
        let mut img_cache = ImageCache::new(8, 1024);
        assert_eq!(img_cache.state("avatar:1"), LazyImageState::Unloaded);

        img_cache.mark_decoding("avatar:1");
        assert_eq!(img_cache.state("avatar:1"), LazyImageState::Decoding);

        let pixels: Arc<[u8]> = Arc::from(vec![255u8; 512]);
        img_cache.store_decoded("avatar:1", 16, 8, pixels);
        assert!(matches!(
            img_cache.state("avatar:1"),
            LazyImageState::Ready {
                width: 16,
                height: 8,
                ..
            }
        ));
        assert_eq!(img_cache.metrics().current_bytes, 512);
    }
}
