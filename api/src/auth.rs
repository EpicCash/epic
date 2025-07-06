// Copyright 2020 The Grin Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::router::{Handler, HandlerObj, ResponseFuture};
use crate::web::response;

use futures::future::ok;
use hyper::header::{HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE};
use hyper::{Request, Response, StatusCode};
use subtle::ConstantTimeEq;

use crate::web::boxed_body;
use bytes::Bytes;
use http_body_util::Full;

lazy_static! {
	pub static ref EPIC_BASIC_REALM: HeaderValue =
		HeaderValue::from_str("Basic realm=EpicAPI").unwrap();
	pub static ref EPIC_FOREIGN_BASIC_REALM: HeaderValue =
		HeaderValue::from_str("Basic realm=EpicForeignAPI").unwrap();
}



// Basic Authentication Middleware
pub struct BasicAuthURIMiddleware {
	api_basic_auth: String,
	basic_realm: &'static HeaderValue,
	target_uri: String,
}

impl BasicAuthURIMiddleware {
	pub fn new(
		api_basic_auth: String,
		basic_realm: &'static HeaderValue,
		target_uri: String,
	) -> BasicAuthURIMiddleware {
		BasicAuthURIMiddleware {
			api_basic_auth,
			basic_realm,
			target_uri,
		}
	}
}

impl Handler<Full<Bytes>> for BasicAuthURIMiddleware {
	fn call(
		&self,
		req: Request<hyper::body::Incoming>,
		mut handlers: Box<dyn Iterator<Item = HandlerObj>>,
	) -> ResponseFuture {
		let next_handler = match handlers.next() {
			Some(h) => h,
			None => return response(StatusCode::INTERNAL_SERVER_ERROR, "no handler found"),
		};
		if req.method().as_str() == "OPTIONS" {
			return next_handler.call(req, handlers);
		}
		let path = req.uri().path();
		// Protect the target_uri and all its subpaths
		if path == self.target_uri || path.starts_with(&(self.target_uri.clone() + "/")) {
			if req.headers().contains_key(AUTHORIZATION)
				&& req.headers()[AUTHORIZATION]
					.as_bytes()
					.ct_eq(&self.api_basic_auth.as_bytes())
					.unwrap_u8() == 1
			{
				next_handler.call(req, handlers)
			} else {
				// Unauthorized 401
				unauthorized_response(&self.basic_realm)
			}
		} else {
			next_handler.call(req, handlers)
		}
	}
}

fn unauthorized_response(basic_realm: &HeaderValue) -> ResponseFuture {
	let body = boxed_body(
		r#"{
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Unauthorized"
            },
            "id": null
        }"#
		.to_string(),
	);
	let response = Response::builder()
		.status(StatusCode::UNAUTHORIZED)
		.header(WWW_AUTHENTICATE, basic_realm)
		.header("content-type", "application/json")
		.body(body)
		.unwrap();
	Box::pin(ok(response))
}
