#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate rocket;

extern crate rocket_etagged_raw_response;

use std::path::Path;
use std::collections::HashMap;

use rocket_etagged_raw_response::{EtaggedRawResponse, EtagMap, EtagIfNoneMatch};

use rocket::request::State;

#[get("/")]
fn view(etag_map: State<EtagMap>, etag_if_none_match: EtagIfNoneMatch) -> EtaggedRawResponse<'static> {
    let path = Path::join(Path::new("examples"), Path::join(Path::new("images"), "image(貓).jpg"));

    EtaggedRawResponse::from_file(etag_map.inner(), etag_if_none_match, path, None::<String>, None).unwrap()
}

fn main() {
    let etag_map = EtagMap::new(HashMap::new());

    rocket::ignite().manage(etag_map).mount("/", routes![view]).launch();
}