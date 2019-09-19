use crate::rocket::fairing::{Fairing, Info, Kind};
use crate::rocket::Rocket;

use crate::{FileEtagCache, KeyEtagCache};

const FAIRING_NAME: &str = "Etagged Raw";

/// The fairing of `EtaggedRawResponse`.
pub struct EtaggedRawResponseFairing {
    pub(crate) custom_callback: Box<dyn Fn() -> usize + Send + Sync + 'static>,
}

impl Fairing for EtaggedRawResponseFairing {
    fn info(&self) -> Info {
        Info {
            name: FAIRING_NAME,
            kind: Kind::Attach,
        }
    }

    fn on_attach(&self, rocket: Rocket) -> Result<Rocket, Rocket> {
        let cache_capacity = (self.custom_callback)();

        let key_cache = KeyEtagCache::new(cache_capacity);
        let file_cache = FileEtagCache::new(cache_capacity);

        Ok(rocket.manage(key_cache).manage(file_cache))
    }
}
