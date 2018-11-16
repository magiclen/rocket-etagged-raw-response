/*!
# Etagged Raw Response for Rocket Framework

This crate provides a response struct used for responding raw data with **Etag** cache.

See `examples`.
*/

pub extern crate mime;
extern crate mime_guess;
extern crate percent_encoding;
extern crate crc_any;
extern crate rocket;
extern crate rocket_etag_if_none_match;

use std::io::{self, Read, ErrorKind, Cursor};
use std::fs::{self, File};
use std::path::Path;
use std::fmt::{self, Debug, Formatter};
use std::sync::Mutex;
use std::collections::HashMap;

use mime::Mime;
use crc_any::CRC;

pub use rocket_etag_if_none_match::{EntityTag, EtagIfNoneMatch};

use rocket::response::{Response, Responder, Result};
use rocket::request::Request;
use rocket::http::{Status, hyper::header::ETag};

/// This map should be managed by a rocket instance.
pub type EtagMap = Mutex<HashMap<String, EntityTag>>;

/// The response struct used for responding raw data with **Etag** cache.
pub struct EtaggedRawResponse<'a> {
    pub data: Box<Read + 'a>,
    pub is_etag_match: bool,
    pub etag: EntityTag,
    pub file_name: String,
    pub content_type: Option<Mime>,
    pub content_length: Option<u64>,
}

impl<'a> Debug for EtaggedRawResponse<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_fmt(format_args!("EtaggedRawResponse {{is_etag_match: {}, etag: {:?}, file_name: {:?}, content_type: {:?}, content_length: {:?}}}", self.is_etag_match, self.etag, self.file_name, self.content_type, self.content_length))
    }
}

impl<'a> Responder<'a> for EtaggedRawResponse<'a> {
    fn respond_to(self, _: &Request) -> Result<'a> {
        let mut response = Response::build();

        if self.is_etag_match {
            response.status(Status::NotModified);
        } else {
            response.header(ETag(self.etag));

            if !self.file_name.is_empty() {
                response.raw_header("Content-Disposition", format!("inline; filename*=UTF-8''{}", percent_encoding::percent_encode(self.file_name.as_bytes(), percent_encoding::QUERY_ENCODE_SET)));
            }

            if let Some(content_type) = self.content_type {
                response.raw_header("Content-Type", content_type.to_string());
            }

            if let Some(content_length) = self.content_length {
                response.raw_header("Content-Length", content_length.to_string());
            }

            response.streamed_body(self.data);
        }

        response.ok()
    }
}

impl<'a> EtaggedRawResponse<'a> {
    /// Create a `EtaggedRawResponse` instance from a path of a file.
    pub fn from_file<P: AsRef<Path>, S: Into<String>>(etag_map: &EtagMap, etag_if_none_match: EtagIfNoneMatch, path: P, file_name: Option<S>, content_type: Option<Mime>) -> io::Result<EtaggedRawResponse<'a>> {
        let path = match path.as_ref().canonicalize() {
            Ok(path) => path,
            Err(e) => Err(e)?
        };

        if !path.is_file() {
            return Err(io::Error::from(ErrorKind::InvalidInput));
        }

        let path_str = path.to_str().unwrap();

        let etag = etag_map.lock().unwrap().get(path_str).map(|etag| { etag.clone() });

        let etag = match etag {
            Some(etag) => etag,
            None => {
                let mut crc64ecma = CRC::crc64ecma();

                let mut buffer = [0u8; 4096];

                let mut file = File::open(&path)?;

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

                drop(file);

                let crc64 = crc64ecma.get_crc();

                let etag = EntityTag::new(true, format!("{:X}", crc64));

                let path_string = path_str.to_string();

                etag_map.lock().unwrap().insert(path_string, etag.clone());

                etag
            }
        };

        let is_etag_match = etag_if_none_match.weak_eq(&etag);

        if is_etag_match {
            Ok(EtaggedRawResponse {
                data: Box::new(Cursor::new(Vec::new())),
                is_etag_match: true,
                etag,
                file_name: String::new(),
                content_type: None,
                content_length: None,
            })
        } else {
            let file_name = match file_name {
                Some(file_name) => {
                    let file_name = file_name.into();
                    file_name
                }
                None => {
                    path.file_name().unwrap().to_str().unwrap().to_string()
                }
            };

            let file_size = match fs::metadata(&path) {
                Ok(metadata) => {
                    Some(metadata.len())
                }
                Err(e) => return Err(e)
            };

            let content_type = match content_type {
                Some(content_type) => content_type,
                None => match path.extension() {
                    Some(extension) => {
                        mime_guess::get_mime_type(extension.to_str().unwrap())
                    }
                    None => mime::APPLICATION_OCTET_STREAM
                }
            };

            let data = Box::from(File::open(&path)?);

            Ok(EtaggedRawResponse {
                data,
                is_etag_match: false,
                etag,
                file_name,
                content_type: Some(content_type),
                content_length: file_size,
            })
        }
    }

    /// Create a `EtaggedRawResponse` instance from a Vec<u8>.
    pub fn from_vec<K: Into<String>, S: Into<String>>(etag_map: &EtagMap, etag_if_none_match: EtagIfNoneMatch, key: S, vec: Vec<u8>, file_name: S, content_type: Option<Mime>) -> io::Result<EtaggedRawResponse<'a>> {
        let key = key.into();

        let etag = etag_map.lock().unwrap().get(&key).map(|etag| { etag.clone() });

        let etag = match etag {
            Some(etag) => etag,
            None => {
                let mut crc64ecma = CRC::crc64ecma();

                crc64ecma.digest(&vec);

                let crc64 = crc64ecma.get_crc();

                let etag = EntityTag::new(true, format!("{:X}", crc64));

                etag_map.lock().unwrap().insert(key, etag.clone());

                etag
            }
        };

        let is_etag_match = etag_if_none_match.weak_eq(&etag);

        if is_etag_match {
            Ok(EtaggedRawResponse {
                data: Box::new(Cursor::new(Vec::new())),
                is_etag_match: true,
                etag,
                file_name: String::new(),
                content_type: None,
                content_length: None,
            })
        } else {
            let file_name = file_name.into();

            let content_length = vec.len();

            Ok(EtaggedRawResponse {
                data: Box::from(Cursor::new(vec)),
                is_etag_match: false,
                etag,
                file_name,
                content_type,
                content_length: Some(content_length as u64),
            })
        }
    }

    /// Create a `EtaggedRawResponse` instance from a reader.
    pub fn from_reader<R: Read + 'a, S: Into<String>>(etag_if_none_match: EtagIfNoneMatch, etag: EntityTag, reader: R, file_name: S, content_type: Option<Mime>, content_length: Option<u64>) -> io::Result<EtaggedRawResponse<'a>> {
        let is_etag_match = etag_if_none_match.weak_eq(&etag);

        if is_etag_match {
            Ok(EtaggedRawResponse {
                data: Box::new(Cursor::new(Vec::new())),
                is_etag_match: true,
                etag,
                file_name: String::new(),
                content_type: None,
                content_length: None,
            })
        } else {
            let file_name = file_name.into();

            Ok(EtaggedRawResponse {
                data: Box::from(reader),
                is_etag_match: false,
                etag,
                file_name,
                content_type,
                content_length,
            })
        }
    }
}