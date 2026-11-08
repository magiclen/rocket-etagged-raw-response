Etagged Raw Response for Rocket Framework
====================

[![CI](https://github.com/magiclen/rocket-etagged-raw-response/actions/workflows/ci.yml/badge.svg)](https://github.com/magiclen/rocket-etagged-raw-response/actions/workflows/ci.yml)

This crate provides a response struct used for responding raw data with **Etag** cache.

```rust
use rocket::get;
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponse};

#[get("/")]
fn index(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("hello.txt")
}
```

The entity-tag is compared with the request's `If-None-Match` header.
A match returns `304 Not Modified` for `GET` or `HEAD` and `412 Precondition Failed` for other methods.
Both responses include the entity-tag and have an empty body.
The entity-tag is also sent when there is no match.

Routes that change data should check the precondition against the current resource before making changes.

The media type is guessed from the extension of the file name, but it can also be set explicitly.

```rust
use rocket::get;
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponse, mime};

#[get("/")]
fn index(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"a,b\n1,2\n".to_vec())
        .file_name("report.csv")
        .content_type(mime::TEXT_CSV)
}
```

A file on disk is opened asynchronously, and its name and media type default to what the path says.
Its weak entity-tag uses the file length and modification time.
Changes that keep both values can reuse the same tag.

```rust
use std::io;

use rocket::get;
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponse};

#[get("/")]
async fn index(etag_if_none_match: EtagIfNoneMatch<'_>) -> io::Result<EtaggedRawResponse> {
    EtaggedRawResponse::from_file(&etag_if_none_match, "images/image.jpg").await
}
```

Nothing can be read twice from a reader, so the entity-tag of a reader has to be provided.

```rust
use std::io::Cursor;

use rocket::get;
use rocket_etagged_raw_response::{EntityTag, EtagIfNoneMatch, EtaggedRawResponse};

#[get("/")]
fn index(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    let etag = EntityTag::with_str(true, "MAGIC").unwrap();

    EtaggedRawResponse::from_reader_sized(
        &etag_if_none_match,
        etag,
        Cursor::new(b"Hello world!"),
        12,
    )
    .file_name("hello.txt")
}
```

An uploaded file can be sent straight back.
Rocket drops the extension from `TempFile::name`, so this crate uses the original file name to recover it and falls back to the upload's media type.

```rust
use std::io;

use rocket::{form::Form, fs::TempFile, post};
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponsePro};

#[post("/", data = "<file>")]
async fn echo<'r>(
    etag_if_none_match: EtagIfNoneMatch<'r>,
    file: Form<TempFile<'r>>,
) -> io::Result<EtaggedRawResponsePro<'r>> {
    EtaggedRawResponsePro::from_temp_file(&etag_if_none_match, file.into_inner()).await
}
```

`attachment` asks the client to save the body instead of displaying it.

```rust
use rocket::get;
use rocket_etagged_raw_response::{EtagIfNoneMatch, EtaggedRawResponse};

#[get("/")]
fn index(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("hello.txt")
        .attachment()
}
```

## Crates.io

https://crates.io/crates/rocket-etagged-raw-response

## Documentation

https://docs.rs/rocket-etagged-raw-response

## License

[MIT](LICENSE)
