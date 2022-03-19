#[macro_use]
extern crate educe;

pub extern crate mime;

mod temp_file_async_reader;

use std::io::{self, Cursor};
use std::marker::Unpin;
use std::path::Path;
use std::sync::Arc;

use mime::Mime;

use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::request::Request;
use rocket::response::{self, Responder, Response};

use rocket::tokio::fs::File as AsyncFile;
use rocket::tokio::io::AsyncRead;

pub use rocket_etag_if_none_match::entity_tag::EntityTag;
pub use rocket_etag_if_none_match::EtagIfNoneMatch;

use temp_file_async_reader::TempFileAsyncReader;

#[derive(Educe)]
#[educe(Debug)]
enum EtaggedRawResponseData<'o> {
    Slice(&'o [u8]),
    Vec(Vec<u8>),
    Reader {
        #[educe(Debug(ignore))]
        data: Box<dyn AsyncRead + Send + Unpin + 'o>,
        content_length: Option<u64>,
    },
    File(Arc<Path>, AsyncFile),
    TempFile(Box<TempFile<'o>>),
}

#[derive(Debug)]
pub struct EtaggedRawResponse<'o> {
    etag: EntityTag<'static>,
    file_name: Option<String>,
    content_type: Option<Mime>,
    data: Option<EtaggedRawResponseData<'o>>,
}

impl<'r, 'o: 'r> EtaggedRawResponse<'o> {
    /// Create a `EtaggedRawResponse` instance from a `&'o [u8]`.
    pub fn from_static<S: Into<String>>(
        etag_if_none_match: &EtagIfNoneMatch,
        data: &'o [u8],
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> EtaggedRawResponse<'o> {
        let etag = EntityTag::from_data(data);

        if etag_if_none_match.weak_eq(&etag) {
            EtaggedRawResponse {
                etag,
                file_name: None,
                content_type: None,
                data: None,
            }
        } else {
            let file_name = file_name.map(|file_name| file_name.into());

            let data = EtaggedRawResponseData::Slice(data);

            EtaggedRawResponse {
                etag,
                file_name,
                content_type,
                data: Some(data),
            }
        }
    }

    /// Create a `EtaggedRawResponse` instance from a `Vec<u8>`.
    pub fn from_vec<S: Into<String>>(
        etag_if_none_match: &EtagIfNoneMatch,
        vec: Vec<u8>,
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> EtaggedRawResponse<'o> {
        let etag = EntityTag::from_data(vec.as_slice());

        if etag_if_none_match.weak_eq(&etag) {
            EtaggedRawResponse {
                etag,
                file_name: None,
                content_type: None,
                data: None,
            }
        } else {
            let file_name = file_name.map(|file_name| file_name.into());

            let data = EtaggedRawResponseData::Vec(vec);

            EtaggedRawResponse {
                etag,
                file_name,
                content_type,
                data: Some(data),
            }
        }
    }

    /// Create a `EtaggedRawResponse` instance from a reader.
    pub fn from_reader<R: AsyncRead + Send + Unpin + 'o, S: Into<String>>(
        etag_if_none_match: &EtagIfNoneMatch,
        etag: EntityTag<'static>,
        reader: R,
        file_name: Option<S>,
        content_type: Option<Mime>,
        content_length: Option<u64>,
    ) -> EtaggedRawResponse<'o> {
        if etag_if_none_match.weak_eq(&etag) {
            EtaggedRawResponse {
                etag,
                file_name: None,
                content_type: None,
                data: None,
            }
        } else {
            let file_name = file_name.map(|file_name| file_name.into());

            let data = EtaggedRawResponseData::Reader {
                data: Box::new(reader),
                content_length,
            };

            EtaggedRawResponse {
                etag,
                file_name,
                content_type,
                data: Some(data),
            }
        }
    }

    /// Create a `EtaggedRawResponse` instance from a path of a file.
    pub async fn from_file<P: Into<Arc<Path>>, S: Into<String>>(
        etag_if_none_match: &EtagIfNoneMatch<'r>,
        path: P,
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> Result<EtaggedRawResponse<'o>, io::Error> {
        let path = path.into();

        let file = AsyncFile::open(path.as_ref()).await?;
        let metadata = file.metadata().await?;

        let etag = EntityTag::from_file_meta(&metadata);

        if etag_if_none_match.weak_eq(&etag) {
            Ok(EtaggedRawResponse {
                etag,
                file_name: None,
                content_type: None,
                data: None,
            })
        } else {
            let file_name = file_name.map(|file_name| file_name.into());

            let data = EtaggedRawResponseData::File(path, file);

            Ok(EtaggedRawResponse {
                etag,
                file_name,
                content_type,
                data: Some(data),
            })
        }
    }

    /// Create a `EtaggedRawResponse` instance from a `TempFile`.
    pub async fn from_temp_file<S: Into<String>>(
        etag_if_none_match: &EtagIfNoneMatch<'r>,
        temp_file: TempFile<'o>,
        file_name: Option<S>,
        content_type: Option<Mime>,
    ) -> Result<EtaggedRawResponse<'o>, io::Error> {
        let etag = match &temp_file {
            TempFile::File {
                path,
                ..
            } => {
                let file = AsyncFile::open(path.as_ref()).await?;
                let metadata = file.metadata().await?;

                EntityTag::from_file_meta(&metadata)
            }
            TempFile::Buffered {
                content,
            } => EntityTag::from_data(content),
        };

        if etag_if_none_match.weak_eq(&etag) {
            Ok(EtaggedRawResponse {
                etag,
                file_name: None,
                content_type: None,
                data: None,
            })
        } else {
            let file_name = file_name.map(|file_name| file_name.into());

            let data = EtaggedRawResponseData::TempFile(Box::new(temp_file));

            Ok(EtaggedRawResponse {
                etag,
                file_name,
                content_type,
                data: Some(data),
            })
        }
    }
}

macro_rules! file_name {
    ($s:expr, $res:expr) => {
        if let Some(file_name) = $s.file_name {
            if !file_name.is_empty() {
                let mut v = String::from("inline; filename*=UTF-8''");

                url_escape::encode_component_to_string(file_name, &mut v);

                $res.raw_header("Content-Disposition", v);
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

impl<'r, 'o: 'r> Responder<'r, 'o> for EtaggedRawResponse<'o> {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'o> {
        let mut response = Response::build();

        match self.data {
            Some(data) => {
                response.raw_header("Etag", self.etag.to_string());

                match data {
                    EtaggedRawResponseData::Slice(data) => {
                        file_name!(self, response);
                        content_type!(self, response);

                        response.sized_body(data.len(), Cursor::new(data));
                    }
                    EtaggedRawResponseData::Vec(data) => {
                        file_name!(self, response);
                        content_type!(self, response);

                        response.sized_body(data.len(), Cursor::new(data));
                    }
                    EtaggedRawResponseData::Reader {
                        data,
                        content_length,
                    } => {
                        file_name!(self, response);
                        content_type!(self, response);

                        if let Some(content_length) = content_length {
                            response.raw_header("Content-Length", content_length.to_string());
                        }

                        response.streamed_body(data);
                    }
                    EtaggedRawResponseData::File(path, file) => {
                        if let Some(file_name) = self.file_name {
                            if !file_name.is_empty() {
                                let mut v = String::from("inline; filename*=UTF-8''");

                                url_escape::encode_component_to_string(file_name, &mut v);

                                response.raw_header("Content-Disposition", v);
                            }
                        } else if let Some(file_name) =
                            path.file_name().map(|file_name| file_name.to_string_lossy())
                        {
                            let mut v = String::from("inline; filename*=UTF-8''");

                            url_escape::encode_component_to_string(file_name, &mut v);

                            response.raw_header("Content-Disposition", v);
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

                        response.sized_body(None, file);
                    }
                    EtaggedRawResponseData::TempFile(file) => {
                        if let Some(file_name) = self.file_name {
                            if file_name.is_empty() {
                                response.raw_header("Content-Disposition", "inline");
                            } else {
                                let mut v = String::from("inline; filename*=UTF-8''");

                                url_escape::encode_component_to_string(file_name, &mut v);

                                response.raw_header("Content-Disposition", v);
                            }
                        } else if let Some(file_name) = file.name() {
                            if file_name.is_empty() {
                                response.raw_header("Content-Disposition", "inline");
                            } else {
                                let mut v = String::from("attachment; filename*=UTF-8''");

                                url_escape::encode_component_to_string(file_name, &mut v);

                                response.raw_header("Content-Disposition", v);
                            }
                        } else {
                            response.raw_header("Content-Disposition", "inline");
                        }

                        if let Some(content_type) = self.content_type {
                            response.raw_header("Content-Type", content_type.to_string());
                        } else if let Some(content_type) = file.content_type() {
                            response.raw_header("Content-Type", content_type.to_string());
                        } else if let Some(extension) =
                            file.name().map(Path::new).and_then(Path::extension)
                        {
                            if let Some(extension) = extension.to_str() {
                                let content_type =
                                    mime_guess::from_ext(extension).first_or_octet_stream();

                                response.raw_header("Content-Type", content_type.to_string());
                            }
                        }

                        response.raw_header("Content-Length", file.len().to_string());

                        response.streamed_body(
                            TempFileAsyncReader::from(file)
                                .map_err(|_| Status::InternalServerError)?,
                        );
                    }
                }
            }
            None => {
                response.status(Status::NotModified);
            }
        }

        response.ok()
    }
}
