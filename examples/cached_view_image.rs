#[macro_use]
extern crate rocket;

use std::{io::ErrorKind, path::Path};

use rocket::http::Status;
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponse};

#[get("/")]
async fn view(etag_if_none_match: EtagIfNoneMatch<'_>) -> Result<EtaggedRawResponse<'_>, Status> {
    let path = Path::join(Path::new("examples"), Path::join(Path::new("images"), "image(貓).jpg"));

    EtaggedRawResponse::from_file(&etag_if_none_match, path, None::<String>, None).await.map_err(
        |err| {
            if err.kind() == ErrorKind::NotFound {
                Status::NotFound
            } else {
                Status::InternalServerError
            }
        },
    )
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![view])
}
