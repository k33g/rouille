// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::borrow::Cow;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::fs::File;
use rustc_serialize;

/// Contains a prototype of a response. Headers are weakly-typed.
///
/// The response is only sent to the client when you return the `RawResponse` object from your
/// request handler. This means that you are free to create as many `RawResponse` objects as you
/// want.
///
/// Contrary to a `Response`, a `RawResponse` may not be HTTP-compliant. Rouille blindly trusts the
/// `RawResponse` and doesn't perform any check. For this reason, you are encouraged to manipulate
/// a `Response` instead of a `RawResponse` when possible.
pub struct RawResponse {
    /// The status code to return to the user.
    pub status_code: u16,

    /// List of headers to be returned in the response.
    ///
    /// Note that important headers such as `Connection` or `Content-Length` will be ignored
    /// from this list.
    // TODO: document precisely which headers
    pub headers: Vec<(Cow<'static, str>, Cow<'static, str>)>,

    /// An opaque type that contains the body of the response.
    pub data: ResponseBody,
}

impl From<Response> for RawResponse {
    fn from(mut response: Response) -> RawResponse {
        // In order to allocate only what we need, we need to calculate the number of headers.
        let num_headers =
            if response.allow.is_some() || response.status_code == 405 { 1 } else { 0 } +
            if response.content_type.is_some() { 1 } else { 0 } +
            if response.location.is_some() { 1 } else { 0 } +
            if response.content_language.is_some() { 1 } else { 0 };

        let headers = {
            let mut headers = Vec::with_capacity(num_headers);

            if let Some(ref allow) = response.allow {
                headers.push(("Allow".into(), allow.join(", ".into()));
            } else if response.status_code == 405 {
                headers.push(("Allow".into(), "".into()));
            }

            if let Some(content_type) = response.content_type {
                headers.push(("Content-Type".into(), content_type));
            }

            if let Some(location) = response.location {
                headers.push(("Location".into(), location));
            }

            if let Some(ref www_authenticate) = response.www_authenticate {

            } else if response.status_code == 401 {
                response.status_code = 403;
            }

            headers
        };

        // Detects bugs with the number of headers calculated above.
        debug_assert_eq!(headers.len(), headers.capacity());

        RawResponse {
            status_code: response.status_code,
            headers: headers,
            data: response.data,
        }
    }
}

/// Contains a prototype of a response. Headers are strongly-typed.
///
/// The response is only sent to the client when you return the `Response` object from your
/// request handler. This means that you are free to create as many `Response` objects as you want.
pub struct Response {
    /// The status code to return to the user.
    pub status_code: u16,

    /// List of methods (`GET`, `POST`, etc.) supported for the target resource.
    ///
    /// A value of `None` indicates that no list will be returned to the client. A value of `Some`
    /// with an empty `Vec` means that no method is allowed ; in other words, the resource is
    /// disabled.
    ///
    /// With a 405 response code, the server must always return a `Allow` header. In this case,
    /// rouille will return an empty list even if you put `None` here.
    pub allow: Option<Vec<Cow<'static, str>>>,

    /// If set, indicates that the same request may result in a different outcome if the request
    /// supplies credentials or different credentials.
    ///
    /// This corresponds to the `WWW-Authenticate` header.
    ///
    /// When the status code is 401, it is mandatory for the server to return a `WWW-Authenticate`
    /// header. In order to be compliant, rouille will automatically turn status code 401 into 403
    /// if you didn't supply this header.
    pub www_authenticate: (),

    pub content_disposition_filename: Option<Cow<'static, str>>,

    pub content_disposition_attachment: bool,

    /// Specifies the MIME type of the content.
    ///
    /// This corresponds to the `Content-Type` header.
    ///
    /// If you don't specify this, the browser may either interpret the data as
    /// `application/octet-stream` or attempt to determine the type of data by analyzing the body.
    /// When the body is not empty, it is strongly recommended that you always specify a
    /// content-type. But in some situations it may not be possible to know what the content-type
    /// is.
    ///
    /// Rouille doesn't check whether the MIME type is valid.
    // TODO: ^ decide whether that's a good idea ; specs say it's strict, see https://tools.ietf.org/html/rfc2046
    pub content_type: Option<Cow<'static, str>>,

    /// 
    ///
    /// This corresponds to the `Location` header.
    pub location: Option<Cow<'static, str>>,

    /// Specifies the language of the content.
    ///
    /// The language must be a *language tag*, as defined by
    /// [RFC 5646](https://tools.ietf.org/html/rfc5646). For example `en-US`. Rouille doesn't check
    /// whether the language tag is valid.
    ///
    /// This corresponds to the `Content-Language` header.
    pub content_language: Option<Cow<'static, str>>,

    pub cache_control: CacheControl,

    /// Specifies whether any intermediate is allowed to transform the response in order to save
    /// space or bandwidth.
    ///
    /// This corresponds to `Cache-Control: no-transform`.
    ///
    /// If the value is `true`, intermediate caches are not allowed to transform the response.
    /// The default value is `false`.
    ///
    /// You are encouraged to only set this to `true` in very specific situations where the
    /// response must match bit-by-bit. Do not set this to `true` just because you are worried
    /// a cache may do something wrong.
    pub no_transform: bool,

    /// An opaque type that contains the body of the response.
    pub data: ResponseBody,
}

impl Response {
    /// Returns true if the status code of this `Response` indicates success.
    ///
    /// This is the range [200-399].
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world");
    /// assert!(response.success());
    /// ```
    #[inline]
    pub fn success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 400
    }

    /// Shortcut for `!response.success()`.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_400();
    /// assert!(response.error());
    /// ```
    #[inline]
    pub fn error(&self) -> bool {
        !self.success()
    }

    /// Builds a `Response` that redirects the user to another URL with a 303 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::redirect("/foo");
    /// ```
    #[inline]
    pub fn redirect(target: &str) -> Response {
        Response {
            status_code: 303,
            headers: vec![("Location".to_owned(), target.to_owned())],
            data: ResponseBody::empty(),
        }
    }

    /// Builds a `Response` that outputs HTML.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("<p>hello <strong>world</strong></p>");
    /// ```
    #[inline]
    pub fn html<D>(content: D) -> Response where D: Into<Vec<u8>> {
        Response {
            status_code: 200,
            content_type: Some("text/html; charset=utf8".into()),
            data: ResponseBody::from_data(content),
        }
    }

    /// Builds a `Response` that outputs SVG.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::svg("<svg xmlns='http://www.w3.org/2000/svg'/>");
    /// ```
    #[inline]
    pub fn svg<D>(content: D) -> Response where D: Into<Vec<u8>> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "image/svg+xml; charset=utf8".to_owned())],
            data: ResponseBody::from_data(content),
        }
    }

    /// Builds a `Response` that outputs plain text.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world");
    /// ```
    #[inline]
    pub fn text<S>(text: S) -> Response where S: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "text/plain; charset=utf8".to_owned())],
            data: ResponseBody::from_string(text),
        }
    }

    /// Builds a `Response` that outputs JSON.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate rustc_serialize;
    /// # #[macro_use] extern crate rouille;
    /// use rouille::Response;
    /// # fn main() {
    ///
    /// #[derive(RustcEncodable)]
    /// struct MyStruct {
    ///     field1: String,
    ///     field2: i32,
    /// }
    ///
    /// let response = Response::json(&MyStruct { field1: "hello".to_owned(), field2: 5 });
    /// // The Response will contain something like `{ field1: "hello", field2: 5 }`
    /// # }
    /// ```
    #[inline]
    pub fn json<T>(content: &T) -> Response where T: rustc_serialize::Encodable {
        let data = rustc_serialize::json::encode(content).unwrap();

        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            data: ResponseBody::from_data(data),
        }
    }

    /// Builds a `Response` that returns a `401 Not Authorized` status
    /// and a `WWW-Authenticate` header.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::basic_http_auth_login_required("realm");
    /// ```
    #[inline]
    pub fn basic_http_auth_login_required(realm: &str) -> Response {
        // TODO: escape the realm
        Response {
            status_code: 401,
            headers: vec![("WWW-Authenticate".to_owned(), format!("Basic realm=\"{}\"", realm))],
            data: ResponseBody::empty(),
        }
    }

    /// Builds an empty `Response` with a 200 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_200();
    /// ```
    #[inline]
    pub fn empty_200() -> Response {
        Response {
            status_code: 200,
            headers: vec![],
            data: ResponseBody::empty()
        }
    }

    /// Builds an empty `Response` with a 400 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_400();
    /// ```
    #[inline]
    pub fn empty_400() -> Response {
        Response {
            status_code: 400,
            headers: vec![],
            data: ResponseBody::empty()
        }
    }

    /// Builds an empty `Response` with a 404 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_404();
    /// ```
    #[inline]
    pub fn empty_404() -> Response {
        Response {
            status_code: 404,
            headers: vec![],
            data: ResponseBody::empty()
        }
    }

    /// Changes the status code of the response.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world").with_status_code(500);
    /// ```
    #[inline]
    pub fn with_status_code(mut self, code: u16) -> Response {
        self.status_code = code;
        self
    }
}

pub enum CacheControl {
    Public {
        max_age: u64,
        must_revalidate: bool,
    },
    Private {
        max_age: u64,
        must_revalidate: bool,
    },
    NoCache {
        max_age: u64,
        must_revalidate: bool,
    },
    NoStore,
}

/// An opaque type that represents the body of a response.
///
/// You can't access the inside of this struct, but you can build one by using one of the provided
/// constructors. 
///
/// # Example
///
/// ```
/// use rouille::ResponseBody;
/// let body = ResponseBody::from_string("hello world");
/// ```
pub struct ResponseBody {
    data: Box<Read + Send>,
    data_length: Option<usize>,
}

impl ResponseBody {
    /// UNSTABLE. Extracts the content of the response. Do not use.
    #[doc(hidden)]
    #[inline]
    pub fn into_inner(self) -> (Box<Read + Send>, Option<usize>) {
        (self.data, self.data_length)
    }

    /// Builds a `ResponseBody` that doesn't return any data.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::empty();
    /// ```
    #[inline]
    pub fn empty() -> ResponseBody {
        ResponseBody {
            data: Box::new(io::empty()),
            data_length: Some(0),
        }
    }

    /// Builds a new `ResponseBody` that will read the data from a `Read`.
    ///
    /// Note that this is suboptimal compared to other constructors because the length
    /// isn't known in advance.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::io;
    /// use std::io::Read;
    /// use rouille::ResponseBody;
    ///
    /// let body = ResponseBody::from_reader(io::stdin().take(128));
    /// ```
    #[inline]
    pub fn from_reader<R>(data: R) -> ResponseBody where R: Read + Send + 'static {
        ResponseBody {
            data: Box::new(data),
            data_length: None,
        }
    }

    /// Builds a new `ResponseBody` that returns the given data.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::from_data(vec![12u8, 97, 34]);
    /// ```
    #[inline]
    pub fn from_data<D>(data: D) -> ResponseBody where D: Into<Vec<u8>> {
        let data = data.into();
        let len = data.len();

        ResponseBody {
            data: Box::new(Cursor::new(data)),
            data_length: Some(len),
        }
    }

    /// Builds a new `ResponseBody` that returns the content of the given file.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rouille::ResponseBody;
    ///
    /// let file = File::open("page.html").unwrap();
    /// let body = ResponseBody::from_file(file);
    /// ```
    #[inline]
    pub fn from_file(file: File) -> ResponseBody {
        let len = file.metadata().map(|metadata| metadata.len() as usize).ok();

        ResponseBody {
            data: Box::new(file),
            data_length: len,
        }
    }

    /// Builds a new `ResponseBody` that returns an UTF-8 string.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::from_string("hello world");
    /// ```
    #[inline]
    pub fn from_string<S>(data: S) -> ResponseBody where S: Into<String> {
        ResponseBody::from_data(data.into().into_bytes())
    }
}
