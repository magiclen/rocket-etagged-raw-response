use std::{
    io::{Cursor, ErrorKind},
    path::Path,
};

use rocket::{
    form::Form,
    fs::TempFile,
    get,
    http::{ContentType, Header, Status},
    local::blocking::Client,
    post, routes,
};
use rocket_etagged_raw_response::{
    EntityTag, EtagIfNoneMatch, EtaggedRawResponse, EtaggedRawResponsePro, mime,
};

const BOUNDARY: &str = "X-BOUNDARY";

#[get("/vec")]
fn vec_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("hello.txt")
}

#[get("/vec-without-name")]
fn vec_without_name_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
}

#[get("/vec-with-empty-name")]
fn vec_with_empty_name_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec()).file_name("")
}

#[get("/slice")]
fn slice_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_slice(&etag_if_none_match, b"Hello world!").file_name("hello.txt")
}

#[get("/reader")]
fn reader_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    let etag = EntityTag::with_str(true, "MAGIC").unwrap();

    EtaggedRawResponse::from_reader(&etag_if_none_match, etag, Cursor::new(b"Hello world!"))
        .file_name("hello.txt")
}

#[get("/reader-sized")]
fn reader_sized_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    let etag = EntityTag::with_str(true, "MAGIC").unwrap();

    EtaggedRawResponse::from_reader_sized(
        &etag_if_none_match,
        etag,
        Cursor::new(b"Hello world!"),
        12,
    )
    .file_name("hello.txt")
}

#[get("/vec-with-content-type")]
fn vec_with_content_type_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("hello.jpg")
        .content_type(mime::TEXT_PLAIN)
}

#[get("/attachment")]
fn attachment_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("hello.txt")
        .attachment()
}

#[get("/apostrophe")]
fn apostrophe_page(etag_if_none_match: EtagIfNoneMatch<'_>) -> EtaggedRawResponse {
    EtaggedRawResponse::from_vec(&etag_if_none_match, b"Hello world!".to_vec())
        .file_name("it's.txt")
}

#[get("/file?<file_name>")]
async fn file_page(
    etag_if_none_match: EtagIfNoneMatch<'_>,
    file_name: Option<&str>,
) -> Result<EtaggedRawResponse, Status> {
    let path = Path::join(Path::new("examples"), Path::join(Path::new("images"), "image(貓).jpg"));

    let response =
        EtaggedRawResponse::from_file(&etag_if_none_match, path).await.map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                Status::NotFound
            } else {
                Status::InternalServerError
            }
        })?;

    Ok(match file_name {
        Some(file_name) => response.file_name(file_name),
        None => response,
    })
}

#[post("/echo", data = "<file>")]
async fn echo_page<'r>(
    etag_if_none_match: EtagIfNoneMatch<'r>,
    file: Form<TempFile<'r>>,
) -> Result<EtaggedRawResponsePro<'r>, Status> {
    EtaggedRawResponsePro::from_temp_file(&etag_if_none_match, file.into_inner())
        .await
        .map_err(|_| Status::InternalServerError)
}

fn client() -> Client {
    Client::tracked(rocket::build().mount("/", routes![
        vec_page,
        vec_without_name_page,
        vec_with_empty_name_page,
        vec_with_content_type_page,
        slice_page,
        reader_page,
        reader_sized_page,
        attachment_page,
        apostrophe_page,
        file_page,
        echo_page
    ]))
    .unwrap()
}

#[test]
fn vec_with_file_name() {
    let client = client();

    let response = client.get("/vec").dispatch();

    assert_eq!(
        Some("inline; filename=\"hello.txt\"; filename*=UTF-8''hello.txt"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("text/plain"), response.headers().get_one("Content-Type"));
    assert_eq!(
        Some(EntityTag::from_data(b"Hello world!").to_string().as_str()),
        response.headers().get_one("ETag")
    );
    assert_eq!(Some(String::from("Hello world!")), response.into_string());
}

/// Nothing says what the body is, so no media type is sent and the client is left to sniff it.
#[test]
fn vec_without_file_name() {
    let client = client();

    let response = client.get("/vec-without-name").dispatch();

    assert_eq!(Some("inline"), response.headers().get_one("Content-Disposition"));
    assert_eq!(None, response.headers().get_one("Content-Type"));
}

/// An empty file name is how a caller asks for a response that carries no name at all.
#[test]
fn vec_with_empty_file_name() {
    let client = client();

    let response = client.get("/vec-with-empty-name").dispatch();

    assert_eq!(Some("inline"), response.headers().get_one("Content-Disposition"));
}

#[test]
fn slice_with_file_name() {
    let client = client();

    let response = client.get("/slice").dispatch();

    assert_eq!(
        Some("inline; filename=\"hello.txt\"; filename*=UTF-8''hello.txt"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("text/plain"), response.headers().get_one("Content-Type"));
    assert_eq!(
        Some(EntityTag::from_data(b"Hello world!").to_string().as_str()),
        response.headers().get_one("ETag")
    );
    assert_eq!(Some(String::from("Hello world!")), response.into_string());
}

/// The length of a plain reader is not known in advance, so no `Content-Length` can be sent.
#[test]
fn reader_without_content_length() {
    let client = client();

    let response = client.get("/reader").dispatch();

    assert_eq!(None, response.headers().get_one("Content-Length"));
    assert_eq!(Some(String::from("Hello world!")), response.into_string());
}

#[test]
fn reader_with_content_length() {
    let client = client();

    let response = client.get("/reader-sized").dispatch();

    assert_eq!(Some("12"), response.headers().get_one("Content-Length"));
    assert_eq!(Some(String::from("Hello world!")), response.into_string());
}

/// A reader cannot be read twice, so the entity-tag is the one the caller has provided.
#[test]
fn reader_uses_the_given_entity_tag() {
    let client = client();

    let response = client.get("/reader").dispatch();

    assert_eq!(Some("W/\"MAGIC\""), response.headers().get_one("ETag"));
}

#[test]
fn explicit_content_type_wins_over_the_extension() {
    let client = client();

    let response = client.get("/vec-with-content-type").dispatch();

    assert_eq!(Some("text/plain"), response.headers().get_one("Content-Type"));
}

#[test]
fn attachment_disposition() {
    let client = client();

    let response = client.get("/attachment").dispatch();

    assert_eq!(
        Some("attachment; filename=\"hello.txt\"; filename*=UTF-8''hello.txt"),
        response.headers().get_one("Content-Disposition")
    );
}

/// `'` is the delimiter of the `filename*` value, so it has to be percent-encoded.
#[test]
fn apostrophe_in_file_name() {
    let client = client();

    let response = client.get("/apostrophe").dispatch();

    assert_eq!(
        Some("inline; filename=\"it's.txt\"; filename*=UTF-8''it%27s.txt"),
        response.headers().get_one("Content-Disposition")
    );
}

/// `(` and `)` are not in the `attr-char` set of RFC 8187, and neither is any byte of a non-ASCII character.
#[test]
fn file_with_a_name_taken_from_the_path() {
    let client = client();

    let response = client.get("/file").dispatch();

    assert_eq!(Status::Ok, response.status());
    assert_eq!(
        Some("inline; filename=\"image(_).jpg\"; filename*=UTF-8''image%28%E8%B2%93%29.jpg"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("image/jpeg"), response.headers().get_one("Content-Type"));
}

#[test]
fn renamed_file_uses_the_source_media_type() {
    let client = client();

    let response = client.get("/file?file_name=report.unknownext").dispatch();

    assert_eq!(Status::Ok, response.status());
    assert_eq!(
        Some("inline; filename=\"report.unknownext\"; filename*=UTF-8''report.unknownext"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("image/jpeg"), response.headers().get_one("Content-Type"));
}

#[test]
fn temp_file_keeps_its_extensions_without_the_client_path() {
    let client = client();

    let body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; \
         filename=\"C:\\fakepath\\report.final.pdf\"\r\nContent-Type: \
         application/pdf\r\n\r\n%PDF-1.4\r\n--{BOUNDARY}--\r\n"
    );

    let response = client
        .post("/echo")
        .header(ContentType::new("multipart", "form-data").with_params(("boundary", BOUNDARY)))
        .body(body)
        .dispatch();

    assert_eq!(Status::Ok, response.status());
    assert_eq!(
        Some("inline; filename=\"report.final.pdf\"; filename*=UTF-8''report.final.pdf"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("application/pdf"), response.headers().get_one("Content-Type"));
    assert_eq!(Some("8"), response.headers().get_one("Content-Length"));
    assert_eq!(Some(String::from("%PDF-1.4")), response.into_string());
}

/// A client that does not know what it is uploading sends `application/octet-stream`, so the media type has to come from the file name instead.
#[test]
fn temp_file_uploaded_without_a_media_type() {
    let client = client();

    let body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; \
         filename=\"report.pdf\"\r\nContent-Type: \
         application/octet-stream\r\n\r\n%PDF-1.4\r\n--{BOUNDARY}--\r\n"
    );

    let response = client
        .post("/echo")
        .header(ContentType::new("multipart", "form-data").with_params(("boundary", BOUNDARY)))
        .body(body)
        .dispatch();

    assert_eq!(Status::Ok, response.status());
    assert_eq!(
        Some("inline; filename=\"report.pdf\"; filename*=UTF-8''report.pdf"),
        response.headers().get_one("Content-Disposition")
    );
    assert_eq!(Some("application/pdf"), response.headers().get_one("Content-Type"));
}

/// A value field of a form arrives as a `TempFile` that is buffered in memory instead of stored on disk, and it carries neither a name nor a media type.
/// Such a body is sent as plain bytes, so its length is left to the server to report just like the one of a `Vec<u8>`.
#[test]
fn buffered_temp_file() {
    let client = client();

    let response =
        client.post("/echo").header(ContentType::Form).body("file=Hello%20world!").dispatch();

    assert_eq!(Status::Ok, response.status());
    assert_eq!(Some("inline"), response.headers().get_one("Content-Disposition"));
    assert_eq!(None, response.headers().get_one("Content-Type"));
    assert_eq!(Some(String::from("Hello world!")), response.into_string());
}

/// A `304 Not Modified` carries no body, but it still has to carry the entity-tag a `200 OK` would have carried.
#[test]
fn not_modified_when_the_entity_tag_matches() {
    let client = client();

    let etag = String::from(client.get("/vec").dispatch().headers().get_one("ETag").unwrap());

    for request in [client.get("/vec"), client.head("/vec")] {
        let response = request.header(Header::new("If-None-Match", etag.clone())).dispatch();

        assert_eq!(Status::NotModified, response.status());
        assert_eq!(Some(etag.as_str()), response.headers().get_one("ETag"));
        assert_eq!(None, response.headers().get_one("Content-Disposition"));
        assert_eq!(None, response.into_string());
    }
}

/// `*` matches whatever representation the server currently has.
#[test]
fn not_modified_when_any_entity_tag_is_requested() {
    let client = client();

    let response = client.get("/vec").header(Header::new("If-None-Match", "*")).dispatch();

    assert_eq!(Status::NotModified, response.status());
}

#[test]
fn post_fails_when_the_entity_tag_matches() {
    let client = client();

    let response =
        client.post("/echo").header(ContentType::Form).body("file=Hello%20world!").dispatch();

    assert_eq!(Status::Ok, response.status());

    let etag = String::from(response.headers().get_one("ETag").unwrap());

    let response = client
        .post("/echo")
        .header(ContentType::Form)
        .header(Header::new("If-None-Match", etag.clone()))
        .body("file=Hello%20world!")
        .dispatch();

    assert_eq!(Status::PreconditionFailed, response.status());
    assert_eq!(Some(etag.as_str()), response.headers().get_one("ETag"));
    assert_eq!(None, response.into_string());
}

/// The entity-tag of a file comes from its metadata, so revalidating an untouched file hits the cache.
#[test]
fn file_is_revalidated() {
    let client = client();

    let response = client.get("/file").dispatch();

    assert_eq!(Status::Ok, response.status());

    let etag = String::from(response.headers().get_one("ETag").unwrap());

    let response =
        client.get("/file").header(Header::new("If-None-Match", etag.clone())).dispatch();

    assert_eq!(Status::NotModified, response.status());
    assert_eq!(Some(etag.as_str()), response.headers().get_one("ETag"));
    assert_eq!(None, response.into_string());
}
