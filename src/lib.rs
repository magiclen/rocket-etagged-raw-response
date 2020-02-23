/*!
# Etagged Raw Response for Rocket Framework

This crate provides a response struct used for responding raw data with **Etag** cache.

See `examples`.
*/

mod fairing;
mod file_etag_cache;
mod key_etag_cache;

#[macro_use]
extern crate educe;
extern crate crc_any;
extern crate lru_time_cache;
pub extern crate mime;
extern crate mime_guess;
extern crate percent_encoding;

extern crate rocket;
extern crate rocket_etag_if_none_match;

use std::io::{Cursor, ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;
use std::fs::File;

use mime::Mime;
use percent_encoding::{AsciiSet, CONTROLS};

use rocket::fairing::Fairing;
use rocket::http::Status;
use rocket::request::Request;
use rocket::response::{self, Responder, Response};
use rocket::State;

use rocket_etag_if_none_match::EtagIfNoneMatch;

pub use rocket_etag_if_none_match::EntityTag;

use fairing::EtaggedRawResponseFairing;
pub use file_etag_cache::FileEtagCache;
pub use key_etag_cache::KeyEtagCache;

const DEFAULT_CACHE_CAPACITY: usize = 64;

const FRAGMENT_PERCENT_ENCODE_SET: &AsciiSet =
    &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

const PATH_PERCENT_ENCODE_SET: &AsciiSet =
    &FRAGMENT_PERCENT_ENCODE_SET.add(b'#').add(b'?').add(b'{').add(b'}');

#[derive(Educe)]
#[educe(Debug)]
enum EtaggedRawResponseData {
    Vec {
        data: Vec<u8>,
        key: Arc<str>,
    },
    Reader {
        #[educe(Debug(ignore))]
        data: Box<dyn Read + 'static>,
        content_length: Option<u64>,
        etag: EntityTag,
    },
    File(Arc<Path>),
}

#[derive(Debug)]
pub struct EtaggedRawResponse {
    file_name: Option<String>,
    content_type: Option<Mime>,
    data: EtaggedRawResponseData,
}

impl EtaggedRawResponse {
    /// Create a `EtaggedRawResponse` instance from a `Vec<u8>`.
    pub fn from_vec<K: Into<Arc<str>>, S: Into<String>>(
        key: K,
        vec: Vec<u8>,
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> EtaggedRawResponse {
        let key = key.into();
        let file_name = file_name.map(|file_name| file_name.into());

        let data = EtaggedRawResponseData::Vec {
            data: vec,
            key,
        };

        EtaggedRawResponse {
            file_name,
            content_type,
            data,
        }
    }

    /// Create a `EtaggedRawResponse` instance from a reader.
    pub fn from_reader<R: Read + 'static, S: Into<String>>(
        etag: EntityTag,
        reader: R,
        file_name: Option<S>,
        content_type: Option<Mime>,
        content_length: Option<u64>,
    ) -> EtaggedRawResponse {
        let file_name = file_name.map(|file_name| file_name.into());

        let data = EtaggedRawResponseData::Reader {
            data: Box::new(reader),
            content_length,
            etag,
        };

        EtaggedRawResponse {
            file_name,
            content_type,
            data,
        }
    }

    /// Create a `EtaggedRawResponse` instance from a path of a file.
    pub fn from_file<P: Into<Arc<Path>>, S: Into<String>>(
        path: P,
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> EtaggedRawResponse {
        let path = path.into();
        let file_name = file_name.map(|file_name| file_name.into());

        let data = EtaggedRawResponseData::File(path);

        EtaggedRawResponse {
            file_name,
            content_type,
            data,
        }
    }
}

impl EtaggedRawResponse {
    #[inline]
    /// Create the fairing of `EtaggedRawResponse`.
    pub fn fairing() -> impl Fairing {
        EtaggedRawResponseFairing {
            custom_callback: Box::new(move || DEFAULT_CACHE_CAPACITY),
        }
    }

    #[inline]
    /// Create the fairing of `EtaggedRawResponse`.
    pub fn fairing_cache<F>(f: F) -> impl Fairing
    where
        F: Fn() -> usize + Send + Sync + 'static, {
        EtaggedRawResponseFairing {
            custom_callback: Box::new(f),
        }
    }
}

macro_rules! file_name {
    ($s:expr, $res:expr) => {
        if let Some(file_name) = $s.file_name {
            if !file_name.is_empty() {
                $res.raw_header(
                    "Content-Disposition",
                    format!(
                        "inline; filename*=UTF-8''{}",
                        percent_encoding::percent_encode(
                            file_name.as_bytes(),
                            PATH_PERCENT_ENCODE_SET
                        )
                    ),
                );
            }
        }
    };
}

macro_rules! content_type {
    ($s:expr, $res:expr) => {
        if let Some(content_type) = $s.content_type {
            $res.raw_header("Content-Type", content_type.to_string());
        }
    };
}

impl<'a> Responder<'a> for EtaggedRawResponse {
    fn respond_to(self, request: &Request) -> response::Result<'a> {
        let client_etag = request.guard::<EtagIfNoneMatch>().unwrap();

        let mut response = Response::build();

        match self.data {
            EtaggedRawResponseData::Vec {
                data,
                key,
            } => {
                let etag_cache = request
                    .guard::<State<KeyEtagCache>>()
                    .expect("KeyEtagCache registered in on_attach");

                let etag = etag_cache.get_or_insert(key.clone(), data.as_slice());

                let is_etag_match = client_etag.weak_eq(&etag);

                if is_etag_match {
                    response.status(Status::NotModified);
                } else {
                    file_name!(self, response);
                    content_type!(self, response);

                    response.raw_header("Etag", etag.to_string());

                    response.sized_body(Cursor::new(data));
                }
            }
            EtaggedRawResponseData::Reader {
                data,
                content_length,
                etag,
            } => {
                let is_etag_match = client_etag.weak_eq(&etag);

                if is_etag_match {
                    response.status(Status::NotModified);
                } else {
                    file_name!(self, response);
                    content_type!(self, response);

                    if let Some(content_length) = content_length {
                        response.raw_header("Content-Length", content_length.to_string());
                    }

                    response.raw_header("Etag", etag.to_string());

                    response.streamed_body(data);
                }
            }
            EtaggedRawResponseData::File(path) => {
                let etag_cache = request
                    .guard::<State<FileEtagCache>>()
                    .expect("FileEtagCache registered in on_attach");

                let etag = etag_cache
                    .get_or_insert(path.clone())
                    .map_err(|_| Status::InternalServerError)?;

                let is_etag_match = client_etag.weak_eq(&etag);

                if is_etag_match {
                    response.status(Status::NotModified);
                } else {
                    if let Some(file_name) = self.file_name {
                        if !file_name.is_empty() {
                            response.raw_header(
                                "Content-Disposition",
                                format!(
                                    "inline; filename*=UTF-8''{}",
                                    percent_encoding::percent_encode(
                                        file_name.as_bytes(),
                                        PATH_PERCENT_ENCODE_SET
                                    )
                                ),
                            );
                        }
                    } else if let Some(file_name) =
                        path.file_name().map(|file_name| file_name.to_string_lossy())
                    {
                        response.raw_header(
                            "Content-Disposition",
                            format!(
                                "inline; filename*=UTF-8''{}",
                                percent_encoding::percent_encode(
                                    file_name.as_bytes(),
                                    PATH_PERCENT_ENCODE_SET
                                )
                            ),
                        );
                    }

                    if let Some(content_type) = self.content_type {
                        response.raw_header("Content-Type", content_type.to_string());
                    } else if let Some(extension) = path.extension() {
                        if let Some(extension) = extension.to_str() {
                            let content_type =
                                mime_guess::from_ext(extension).first_or_octet_stream();

                            response.raw_header("Content-Type", content_type.to_string());
                        }
                    }

                    let file = File::open(path).map_err(|err| {
                        if err.kind() == ErrorKind::NotFound {
                            Status::NotFound
                        } else {
                            Status::InternalServerError
                        }
                    })?;

                    response.raw_header("Etag", etag.to_string());

                    response.sized_body(file);
                }
            }
        }

        response.ok()
    }
}
