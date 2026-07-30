/*!
# Etagged Raw Response for Rocket Framework

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

The entity-tag is computed from the data and is compared with the `if-none-match` header of the request. A `304 Not Modified` with no body is responded when they match, and the entity-tag is sent either way.

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

A file on disk is opened asynchronously, its entity-tag comes from its metadata instead of its content, and both the file name and the media type default to what the path says.

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

An uploaded file can be sent straight back. Rocket drops the extension from `TempFile::name`, so this crate puts it back from the media type of the upload.

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
*/

pub extern crate mime;
pub extern crate rocket;

mod temp_file_async_reader;

use std::{
    borrow::Cow,
    ffi::OsStr,
    io::{self, Cursor},
    path::Path,
};

use educe::Educe;
use mime::Mime;
use rocket::{
    fs::TempFile,
    http::Status,
    request::Request,
    response::{self, Builder, Responder, Response},
    tokio::{fs::File as AsyncFile, io::AsyncRead},
};
pub use rocket_etag_if_none_match::{EtagIfNoneMatch, entity_tag::EntityTag};
use temp_file_async_reader::TempFileAsyncReader;

#[derive(Educe)]
#[educe(Debug)]
enum EtaggedRawResponseData<'o> {
    Bytes(Cow<'o, [u8]>),
    Reader {
        #[educe(Debug(ignore))]
        data:           Box<dyn AsyncRead + Send + Unpin + 'o>,
        content_length: Option<u64>,
    },
    File {
        path: Box<Path>,
        file: AsyncFile,
        /// Read from the metadata the entity-tag was computed from, so the length of the body does not have to be found again by seeking.
        len:  u64,
    },
    TempFile {
        temp_file:  Box<TempFile<'o>>,
        async_file: AsyncFile,
        /// Read from the metadata the entity-tag was computed from, so the length of the body does not have to be found again by seeking.
        len:        u64,
    },
}

/// An [`EtaggedRawResponsePro`] that borrows nothing, which is what a route usually returns.
pub type EtaggedRawResponse = EtaggedRawResponsePro<'static>;

/// The responder that makes a client display the body as it is, and that answers `304 Not Modified` when the client already holds the very same body.
#[derive(Debug)]
pub struct EtaggedRawResponsePro<'o> {
    etag:         EntityTag<'static>,
    /// `None` once the entity-tag has matched the `if-none-match` header, which is what turns the response into a `304 Not Modified`.
    data:         Option<EtaggedRawResponseData<'o>>,
    file_name:    Option<String>,
    content_type: Option<Mime>,
    inline:       bool,
}

impl<'o> EtaggedRawResponsePro<'o> {
    /// `data` is not built at all when the entity-tag matches, so nothing is allocated for a body that is not going to be sent.
    #[inline]
    fn new<F: FnOnce() -> EtaggedRawResponseData<'o>>(
        etag_if_none_match: &EtagIfNoneMatch<'_>,
        etag: EntityTag<'static>,
        data: F,
    ) -> Self {
        let data = if etag_if_none_match.weak_eq(&etag) { None } else { Some(data()) };

        EtaggedRawResponsePro {
            etag,
            data,
            file_name: None,
            content_type: None,
            inline: true,
        }
    }

    /// Create an `EtaggedRawResponse` instance from a `&'o [u8]`.
    #[inline]
    pub fn from_slice(etag_if_none_match: &EtagIfNoneMatch<'_>, data: &'o [u8]) -> Self {
        let etag = EntityTag::from_data(data);

        Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::Bytes(Cow::Borrowed(data)))
    }

    /// Create an `EtaggedRawResponse` instance from a `Vec<u8>`.
    #[inline]
    pub fn from_vec(etag_if_none_match: &EtagIfNoneMatch<'_>, vec: Vec<u8>) -> Self {
        let etag = EntityTag::from_data(vec.as_slice());

        Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::Bytes(Cow::Owned(vec)))
    }

    /// Create an `EtaggedRawResponse` instance from a reader whose length is not known in advance, so no `Content-Length` header is sent. The entity-tag has to be provided because a reader cannot be read twice.
    #[inline]
    pub fn from_reader<R: AsyncRead + Send + Unpin + 'o>(
        etag_if_none_match: &EtagIfNoneMatch<'_>,
        etag: EntityTag<'static>,
        reader: R,
    ) -> Self {
        Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::Reader {
            data:           Box::new(reader),
            content_length: None,
        })
    }

    /// Create an `EtaggedRawResponse` instance from a reader whose length is already known, so a `Content-Length` header can be sent. The entity-tag has to be provided because a reader cannot be read twice.
    #[inline]
    pub fn from_reader_sized<R: AsyncRead + Send + Unpin + 'o>(
        etag_if_none_match: &EtagIfNoneMatch<'_>,
        etag: EntityTag<'static>,
        reader: R,
        content_length: u64,
    ) -> Self {
        Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::Reader {
            data:           Box::new(reader),
            content_length: Some(content_length),
        })
    }

    /// Create an `EtaggedRawResponse` instance from a path of a file, which is opened right away. The entity-tag is computed from the metadata of the file rather than from its content.
    pub async fn from_file<P: AsRef<Path>>(
        etag_if_none_match: &EtagIfNoneMatch<'_>,
        path: P,
    ) -> Result<Self, io::Error> {
        let path = path.as_ref();

        let file = AsyncFile::open(path).await?;
        // The metadata is read from the very handle the body is going to be read from, so the entity-tag cannot end up describing another version of the file.
        let metadata = file.metadata().await?;

        let etag = EntityTag::from_file_meta(&metadata);

        Ok(Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::File {
            path: Box::from(path),
            file,
            len: metadata.len(),
        }))
    }

    /// Create an `EtaggedRawResponse` instance from a `TempFile`, which is opened right away when it is stored on disk. The entity-tag is computed from the metadata of such a file rather than from its content.
    ///
    /// A `304 Not Modified` is answered whenever the entity-tag matches, even though a request whose method is neither `GET` nor `HEAD` is meant to be answered with a `412 Precondition Failed` instead.
    pub async fn from_temp_file(
        etag_if_none_match: &EtagIfNoneMatch<'_>,
        temp_file: TempFile<'o>,
    ) -> Result<Self, io::Error> {
        let (etag, async_file, len) = match &temp_file {
            TempFile::File {
                path, ..
            } => {
                let async_file = AsyncFile::open(path).await?;
                // The metadata is read from the very handle the body is going to be read from, so the entity-tag cannot end up describing another version of the file.
                let metadata = async_file.metadata().await?;

                (EntityTag::from_file_meta(&metadata), async_file, metadata.len())
            },
            // A `TempFile` buffered in memory has no name, no media type and no file on disk, so nothing but its content is worth keeping.
            TempFile::Buffered {
                content,
            } => {
                let etag = EntityTag::from_data(content);

                return Ok(Self::new(etag_if_none_match, etag, || {
                    EtaggedRawResponseData::Bytes(Cow::Borrowed(content))
                }));
            },
        };

        Ok(Self::new(etag_if_none_match, etag, || EtaggedRawResponseData::TempFile {
            temp_file: Box::new(temp_file),
            async_file,
            len,
        }))
    }

    /// Set the file name the client should display the body as. An empty name sends no name at all.
    #[inline]
    #[must_use]
    pub fn file_name<S: Into<String>>(mut self, file_name: S) -> Self {
        self.file_name = Some(file_name.into());

        self
    }

    /// Set the media type of the body, which otherwise is guessed from the file name.
    #[inline]
    #[must_use]
    pub fn content_type(mut self, content_type: Mime) -> Self {
        self.content_type = Some(content_type);

        self
    }

    /// Send `attachment` instead of `inline`, which asks the client to save the body rather than to display it.
    #[inline]
    #[must_use]
    pub fn attachment(mut self) -> Self {
        self.inline = false;

        self
    }
}

const OCTET_STREAM: &str = "application/octet-stream";

const HEX_DIGITS: [u8; 16] = *b"0123456789ABCDEF";

/// Percent-encode `s` with the `attr-char` set of RFC 8187, which is the only set the `filename*` parameter accepts.
fn encode_attr_char_to_string(s: &str, output: &mut String) {
    for b in s.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
        {
            output.push(b as char);
        } else {
            output.push('%');
            output.push(HEX_DIGITS[usize::from(b >> 4)] as char);
            output.push(HEX_DIGITS[usize::from(b & 0x0F)] as char);
        }
    }
}

/// Write `s` as the quoted-string of the `filename` parameter, replacing everything a quoted-string cannot hold with `_`.
fn encode_ascii_fallback_to_string(s: &str, output: &mut String) {
    for c in s.chars() {
        if c == ' ' || (c.is_ascii_graphic() && c != '"' && c != '\\') {
            output.push(c);
        } else {
            output.push('_');
        }
    }
}

const FILE_NAME_PREFIX: &str = "; filename=\"";
const FILE_NAME_STAR_PREFIX: &str = "\"; filename*=UTF-8''";

/// Write the `Content-Disposition` header, carrying the file name both as an ASCII fallback and as RFC 8187 UTF-8.
fn write_content_disposition(response: &mut Builder<'_>, inline: bool, file_name: Option<&str>) {
    let disposition = if inline { "inline" } else { "attachment" };

    let Some(file_name) = file_name.filter(|file_name| !file_name.is_empty()) else {
        // The disposition on its own is a `&'static str`, so no string has to be built for it.
        response.raw_header("Content-Disposition", disposition);

        return;
    };

    // The ASCII fallback keeps the length of the name, the RFC 8187 copy triples it at worst.
    let mut v = String::with_capacity(
        disposition.len()
            + FILE_NAME_PREFIX.len()
            + FILE_NAME_STAR_PREFIX.len()
            + file_name.len() * 4,
    );

    v.push_str(disposition);
    v.push_str(FILE_NAME_PREFIX);
    encode_ascii_fallback_to_string(file_name, &mut v);
    v.push_str(FILE_NAME_STAR_PREFIX);
    encode_attr_char_to_string(file_name, &mut v);

    response.raw_header("Content-Disposition", v);
}

/// Write the `Content-Type` header, preferring the explicit media type, then the one the source carries, then a guess made from the file name or from the path of the source.
fn write_content_type(
    response: &mut Builder<'_>,
    content_type: Option<Mime>,
    data: &EtaggedRawResponseData<'_>,
    file_name: Option<&str>,
) {
    if let Some(content_type) = content_type {
        response.raw_header("Content-Type", content_type.to_string());

        return;
    }

    if let EtaggedRawResponseData::TempFile {
        temp_file, ..
    } = data
        && let Some(content_type) = temp_file.content_type()
        // `application/octet-stream` is what a client sends when it does not know either, so a guess does better.
        && !content_type.is_binary()
    {
        response.raw_header("Content-Type", content_type.to_string());

        return;
    }

    let extension = file_name.map(Path::new).and_then(Path::extension).or_else(|| match data {
        // The file name may have been replaced with one that carries no extension, but the path still knows what the file is.
        EtaggedRawResponseData::File {
            path, ..
        } => path.extension(),
        _ => None,
    });

    // Nothing says what the body is. Send no media type at all and let the client sniff it, rather than send `application/octet-stream` and make the client save a body that is meant to be displayed.
    if let Some(extension) = extension.and_then(OsStr::to_str) {
        // A guess is a `&'static str`, so the header does not have to be built from an owned `Mime`.
        let content_type = mime_guess::from_ext(extension).first_raw().unwrap_or(OCTET_STREAM);

        response.raw_header("Content-Type", content_type);
    }
}

/// Rocket strips every extension from `TempFile::name`, so append back the one the client sent, or the one that belongs to the uploaded media type.
fn temp_file_name(temp_file: &TempFile<'_>) -> Option<String> {
    let name = temp_file.name()?;

    // Only the extension is read from the raw name, so any path a client may have sent is still dropped.
    let raw_extension = temp_file.raw_name().and_then(|raw_name| {
        Path::new(raw_name.dangerous_unsafe_unsanitized_raw().as_str()).extension()?.to_str()
    });

    if let Some(extension) = raw_extension {
        return Some(format!("{name}.{extension}"));
    }

    match temp_file.content_type().and_then(|content_type| content_type.extension()) {
        Some(extension) => Some(format!("{name}.{extension}")),
        None => Some(String::from(name)),
    }
}

/// The file name the source suggests, used when the caller sets none.
fn default_file_name(data: &EtaggedRawResponseData<'_>) -> Option<String> {
    match data {
        EtaggedRawResponseData::File {
            path, ..
        } => path.file_name().map(|file_name| file_name.to_string_lossy().into_owned()),
        EtaggedRawResponseData::TempFile {
            temp_file, ..
        } => temp_file_name(temp_file),
        _ => None,
    }
}

impl<'r, 'o: 'r> Responder<'r, 'o> for EtaggedRawResponsePro<'o> {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'o> {
        let EtaggedRawResponsePro {
            etag,
            data,
            file_name,
            content_type,
            inline,
        } = self;

        let mut response = Response::build();

        // A `304 Not Modified` has to carry the entity-tag a `200 OK` would have carried, otherwise a cache cannot tell which representation it has just revalidated.
        response.raw_header(EntityTag::HEADER_NAME, etag.to_string());

        let Some(data) = data else {
            response.status(Status::NotModified);

            return response.ok();
        };

        let file_name = file_name.or_else(|| default_file_name(&data));

        write_content_disposition(&mut response, inline, file_name.as_deref());
        write_content_type(&mut response, content_type, &data, file_name.as_deref());

        match data {
            EtaggedRawResponseData::Bytes(data) => {
                response.sized_body(data.len(), Cursor::new(data));
            },
            EtaggedRawResponseData::Reader {
                data,
                content_length,
            } => {
                if let Some(content_length) = content_length {
                    response.raw_header("Content-Length", content_length.to_string());
                }

                response.streamed_body(data);
            },
            EtaggedRawResponseData::File {
                file,
                len,
                ..
            } => {
                // The length is already known, so it does not have to be found again by seeking to the end of the file. A length that does not fit in a `usize` is left to be handled that way.
                response.sized_body(usize::try_from(len).ok(), file);
            },
            EtaggedRawResponseData::TempFile {
                temp_file,
                async_file,
                len,
            } => {
                response.raw_header("Content-Length", len.to_string());

                response.streamed_body(TempFileAsyncReader::new(temp_file, async_file));
            },
        }

        response.ok()
    }
}
