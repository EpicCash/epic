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

//! RESTful API server to easily expose services as RESTful JSON/HTTP endpoints.
//! Fairly constrained on what the service API must look like by design.
//!
//! To use it, just have your service(s) implement the ApiEndpoint trait and
//! register them on a ApiServer.

use crate::router::{Handler, HandlerObj, ResponseFuture, Router, RouterError};
use crate::web::response;

use hyper::server::conn::AddrIncoming;
use hyper::service::make_service_fn;
use hyper::{Body, Request, Server, StatusCode};
use hyper_rustls::TlsAcceptor;

use std::convert::Infallible;
use std::fs::File;

use std::net::SocketAddr;
use std::sync::Arc;
use std::{io, thread};

/// Errors that can be returned by an ApiEndpoint implementation.
#[derive(Clone, Eq, PartialEq, Debug, thiserror::Error, Serialize, Deserialize)]
pub enum Error {
	#[error("Internal error: {0}")]
	Internal(String),
	#[error("Bad arguments: {0}")]
	Argument(String),
	#[error("Not found.")]
	NotFound,
	#[error("Request error: {0}")]
	RequestError(String),
	#[error("ResponseError error: {0}")]
	ResponseError(String),
	#[error("Router error: {source}")]
	Router {
		#[from]
		source: RouterError,
	},
}

impl From<hyper::http::Error> for Error {
	fn from(error: hyper::http::Error) -> Error {
		Error::RequestError(error.to_string())
	}
}

impl From<crate::chain::Error> for Error {
	fn from(error: crate::chain::Error) -> Error {
		Error::Internal(error.to_string())
	}
}

/// TLS config
#[derive(Clone)]
pub struct TLSConfig {
	pub certificate: String,
	pub private_key: String,
}

impl TLSConfig {
	pub fn new(certificate: String, private_key: String) -> TLSConfig {
		TLSConfig {
			certificate,
			private_key,
		}
	}

	fn load_certs(&self) -> Vec<rustls::Certificate> {
		let certfile = File::open(&self.certificate)
			.expect(&format!("failed to open file {}", self.certificate));
		let mut reader = io::BufReader::new(certfile);
		rustls_pemfile::certs(&mut reader)
			.unwrap()
			.iter()
			.map(|v| rustls::Certificate(v.clone()))
			.collect()
	}

	fn load_private_key(&self) -> rustls::PrivateKey {
		let keyfile = File::open(&self.private_key).expect("cannot open private key file");
		let mut reader = io::BufReader::new(keyfile);

		loop {
			match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file")
			{
				Some(rustls_pemfile::Item::RSAKey(key)) => return rustls::PrivateKey(key),
				Some(rustls_pemfile::Item::PKCS8Key(key)) => return rustls::PrivateKey(key),
				Some(rustls_pemfile::Item::ECKey(key)) => return rustls::PrivateKey(key),
				None => break,
				_ => {}
			}
		}

		panic!(
			"no keys found in {:?} (encrypted keys not supported)",
			&self.private_key
		);
	}

	pub fn build_server_config(&self) -> Result<Arc<rustls::ServerConfig>, Error> {
		let certs = self.load_certs();
		let key = self.load_private_key();
		let cfg = rustls::ServerConfig::builder()
			.with_safe_defaults()
			.with_no_client_auth()
			.with_single_cert(certs, key)
			.expect("bad certificate/key");

		Ok(Arc::new(cfg))
	}
}

/// HTTP server allowing the registration of ApiEndpoint implementations.
pub struct ApiServer {
	shutdown_sender: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ApiServer {
	/// Creates a new ApiServer that will serve ApiEndpoint implementations
	/// under the root URL.
	pub fn new() -> ApiServer {
		ApiServer {
			shutdown_sender: None,
		}
	}

	/// Starts ApiServer at the provided address.
	pub fn start(
		&mut self,
		addr: SocketAddr,
		router: Router,
		conf: Option<TLSConfig>,
		api_chan: &'static mut (
			tokio::sync::oneshot::Sender<()>,
			tokio::sync::oneshot::Receiver<()>,
		),
	) -> Result<thread::JoinHandle<()>, Error> {
		match conf {
			Some(conf) => self.start_tls(addr, router, conf, api_chan),
			None => self.start_no_tls(addr, router, api_chan),
		}
	}

	/// Starts the ApiServer at the provided address.
	fn start_no_tls(
		&mut self,
		addr: SocketAddr,
		router: Router,
		api_chan: &'static mut (
			tokio::sync::oneshot::Sender<()>,
			tokio::sync::oneshot::Receiver<()>,
		),
	) -> Result<thread::JoinHandle<()>, Error> {
		if self.shutdown_sender.is_some() {
			return Err(Error::Internal(
				"Can't start HTTP API server, it's running already".to_string(),
			));
		}

		let tx = &mut api_chan.0;
		let rx = &mut api_chan.1;
		let m = tokio::sync::oneshot::channel::<()>();
		let tx = std::mem::replace(tx, m.0);
		self.shutdown_sender = Some(tx);

		thread::Builder::new()
			.name("apis".to_string())
			.spawn(move || {
				let task = async {
					let server = Server::bind(&addr)
						.serve(make_service_fn(move |_| {
							let router = router.clone();
							async { Ok::<_, Infallible>(router) }
						}))
						.with_graceful_shutdown(async {
							rx.await.ok();
						});

					if let Err(e) = server.await {
						error!("HTTP API server error: {}", e);
					}
				};

				let rt = tokio::runtime::Builder::new_multi_thread()
					.enable_all()
					.build()
					.unwrap();
				rt.block_on(task);
			})
			.map_err(|_| Error::Internal("failed to spawn API thread".to_string()))
	}

	/// Starts the TLS ApiServer at the provided address.
	fn start_tls(
		&mut self,
		addr: SocketAddr,
		router: Router,
		conf: TLSConfig,
		api_chan: &'static mut (
			tokio::sync::oneshot::Sender<()>,
			tokio::sync::oneshot::Receiver<()>,
		),
	) -> Result<thread::JoinHandle<()>, Error> {
		if self.shutdown_sender.is_some() {
			return Err(Error::Internal(
				"Can't start HTTPS API server, it's running already".to_string(),
			));
		}

		let tx = &mut api_chan.0;
		let rx = &mut api_chan.1;

		let m = tokio::sync::oneshot::channel::<()>();
		let tx = std::mem::replace(tx, m.0);
		self.shutdown_sender = Some(tx);

		thread::Builder::new()
			.name("apis".to_string())
			.spawn(move || {
				let task = async {
					let incoming = AddrIncoming::bind(&addr).unwrap();
					let server_config = conf.build_server_config().unwrap();
					let acceptor = TlsAcceptor::new(server_config.to_owned(), incoming);

					let service = make_service_fn(move |_| {
						let router = router.clone();
						async move { Ok::<_, Infallible>(router) }
					});
					let server = Server::builder(acceptor)
						.serve(service)
						.with_graceful_shutdown(async {
							rx.await.ok();
						});

					if let Err(e) = server.await {
						error!("HTTPS API server error: {}", e);
					}
				};
				let rt = tokio::runtime::Builder::new_multi_thread()
					.enable_all()
					.build()
					.unwrap();
				rt.block_on(task);
			})
			.map_err(|_| Error::Internal("failed to spawn API thread".to_string()))
	}

	/// Stops the API server, it panics in case of error
	pub fn stop(&mut self) -> bool {
		if self.shutdown_sender.is_some() {
			// TODO re-enable stop after investigation
			if let Some(tx) = self.shutdown_sender.take() {
				tx.send(()).expect("Failed to stop API server");
				info!("API server has been stopped");
			}

			true
		} else {
			error!("Can't stop API server, it's not running or doesn't support stop operation");
			false
		}
	}
}

pub struct LoggingMiddleware {}

impl Handler for LoggingMiddleware {
	fn call(
		&self,
		req: Request<Body>,
		mut handlers: Box<dyn Iterator<Item = HandlerObj>>,
	) -> ResponseFuture {
		debug!("REST call: {} {}", req.method(), req.uri().path());

		match handlers.next() {
			Some(handler) => handler.call(req, handlers),
			None => response(StatusCode::INTERNAL_SERVER_ERROR, "no handler found"),
		}
	}
}
