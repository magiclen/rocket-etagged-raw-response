#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

extern crate rocket_etagged_raw_response;

use std::path::Path;

use rocket_etagged_raw_response::{EtaggedRawResponse, EtagIfNoneMatch};

#[get("/")]
fn view(etag_if_none_match: EtagIfNoneMatch) -> EtaggedRawResponse {
    let path = Path::join(Path::new("examples"), Path::join(Path::new("images"), "image(貓).jpg"));

    EtaggedRawResponse::from_file(etag_if_none_match, path, None::<String>, None)
}

fn main() {
    rocket::ignite().attach(EtaggedRawResponse::fairing()).mount("/", routes![view]).launch();
}