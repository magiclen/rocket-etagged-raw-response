use std::sync::{Arc, Mutex};

use crate::EntityTag;
use crate::crc_any::CRCu64;
use crate::lru_time_cache::LruCache;

#[inline]
fn compute_data_etag<B: AsRef<[u8]> + ?Sized>(data: &B) -> EntityTag {
    let mut crc64ecma = CRCu64::crc64();
    crc64ecma.digest(data.as_ref());
    let crc64 = crc64ecma.get_crc();
    EntityTag::new(true, format!("{:X}", crc64))
}

#[derive(Educe)]
#[educe(Debug)]
pub struct KeyEtagCache {
    #[educe(Debug(ignore))]
    cache_table: Mutex<LruCache<Arc<str>, Arc<EntityTag>>>,
}

impl KeyEtagCache {
    #[inline]
    /// Create an instance of `EtagCache`.
    pub fn new(cache_capacity: usize) -> KeyEtagCache {
        KeyEtagCache {
            cache_table: Mutex::new(LruCache::with_capacity(cache_capacity)),
        }
    }

    #[inline]
    /// Clear cache.
    pub fn clear_cache(&self) {
        self.cache_table.lock().unwrap().clear();
    }

    #[inline]
    /// Check if a cache key exists.
    pub fn contains_key<S: AsRef<str>>(&self, key: S) -> bool {
        self.cache_table.lock().unwrap().get(key.as_ref()).is_some()
    }

    #[inline]
    /// Get an Etag with a key.
    pub fn get_or_insert<S: Into<Arc<str>>, B: AsRef<[u8]> + ?Sized>(&self, key: S, data: &B) -> Arc<EntityTag> {
        let key = key.into();

        match self.cache_table.lock().unwrap().get(key.as_ref()).map(|etag| etag.clone()) {
            Some(etag) => etag,
            None => {
                let etag = Arc::new(compute_data_etag(data));

                self.cache_table.lock().unwrap().insert(key, etag.clone());

                etag
            }
        }
    }
}