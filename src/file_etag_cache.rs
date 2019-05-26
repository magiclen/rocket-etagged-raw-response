use std::sync::{Arc, Mutex};
use std::path::Path;
use std::io::{self, Read};
use std::fs::File;
use std::time::SystemTime;

use crate::EntityTag;
use crate::crc_any::CRC;
use crate::lru_time_cache::LruCache;

#[inline]
fn compute_file_etag<P: AsRef<Path>>(path: P) -> Result<EntityTag, io::Error> {
    let mut crc64ecma = CRC::crc64ecma();

    let mut buffer = [0u8; 4096];

    {
        let mut file = File::open(path.as_ref())?;

        loop {
            match file.read(&mut buffer) {
                Ok(c) => {
                    if c == 0 {
                        break;
                    }
                    crc64ecma.digest(&buffer[0..c]);
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }
    }

    let crc64 = crc64ecma.get_crc();

    Ok(EntityTag::new(true, format!("{:X}", crc64)))
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct FileEtagCache {
    #[derivative(Debug = "ignore")]
    cache_table: Mutex<LruCache<Arc<Path>, (Arc<EntityTag>, Option<SystemTime>)>>,
}

impl FileEtagCache {
    #[inline]
    /// Create an instance of `EtagCache`.
    pub fn new(cache_capacity: usize) -> FileEtagCache {
        FileEtagCache {
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
    pub fn contains_key<S: AsRef<Path>>(&self, key: S) -> bool {
        self.cache_table.lock().unwrap().get(key.as_ref()).is_some()
    }

    #[inline]
    /// Get an Etag with a name as its key.
    pub fn get_or_insert<P: Into<Arc<Path>> + ?Sized>(&self, path: P) -> io::Result<Arc<EntityTag>> {
        let path = path.into();

        let mtime = match self.cache_table.lock().unwrap().get(path.as_ref()).map(|(etag, mtime)| (etag.clone(), mtime.clone())) {
            Some((etag, mtime)) => {
                let metadata = path.metadata()?;

                match mtime {
                    Some(mtime) => {
                        match metadata.modified() {
                            Ok(new_mtime) => {
                                if new_mtime != mtime {
                                    Some(new_mtime)
                                } else {
                                    return Ok(etag);
                                }
                            }
                            Err(_) => {
                                None
                            }
                        }
                    }
                    None => {
                        match metadata.modified() {
                            Ok(new_mtime) => {
                                Some(new_mtime)
                            }
                            Err(_) => {
                                None
                            }
                        }
                    }
                }
            }
            None => {
                let metadata = path.metadata()?;

                match metadata.modified() {
                    Ok(new_mtime) => {
                        Some(new_mtime)
                    }
                    Err(_) => {
                        None
                    }
                }
            }
        };

        let etag = compute_file_etag(&path)?;

        let etag = Arc::new(etag);

        self.cache_table.lock().unwrap().insert(path, (etag.clone(), mtime));

        Ok(etag)
    }
}