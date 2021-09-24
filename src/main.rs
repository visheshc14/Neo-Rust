#![allow(non_snake_case)]
use async_stream::stream;
use core::task::{Context, Poll};
use env_logger;
use futures_util::stream::Stream;
use hyper::body::Bytes;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use log::LevelFilter;
use rustls::internal::msgs::enums::AlertDescription;
use rustls::TLSError;
use rustls::{Certificate, PrivateKey};
use rustls_pemfile::{read_one, Item};
use std::convert::Infallible;
use std::fs;
use std::io;
use std::io::Read;
use std::iter;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::process;
use std::sync::Arc;
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

#[derive(StructOpt, Debug)]
#[structopt(name = "Neo")]
struct NeoOpts {
    /// Host
    #[structopt(short = "h", long = "host", default_value = "127.0.0.1", env = "HOST")]
    host: String,

    /// Port
    #[structopt(short = "p", long = "port", default_value = "5000", env = "PORT")]
    port: i32,

    /// Amount of seconds to wait for input on STDIN to serve
    #[structopt(
        long = "stdin-read-timeout-seconds",
        default_value = "60",
        env = "STDIN_READ_TIMEOUT_SECONDS"
    )]
    stdin_read_timeout_seconds: u64,

    /// File to read
    #[structopt(
        name = "FILE",
        short = "f",
        long = "file",
        parse(from_os_str),
        env = "FILE"
    )]
    file_path: Option<PathBuf>,

    /////////
    // TLS //
    /////////
    /// TLS key path to use
    #[structopt(
        name = "TLS_KEY",
        long = "tls-key",
        parse(from_os_str),
        env = "TLS_KEY"
    )]
    tls_key_path: Option<PathBuf>,

    /// TLS (CA) cert path to use
    #[structopt(
        name = "TLS_CERT",
        long = "tls-cert",
        parse(from_os_str),
        env = "TLS_CERT"
    )]
    tls_cert_path: Option<PathBuf>,
}

/// Utility function fo serving static content
async fn serve_static_content(
    req: Request<Body>,
    content: Bytes,
) -> Result<Response<Body>, Infallible> {
    match req.method() {
        // Serve the content for every GET request
        &Method::GET => Ok(Response::new(Body::from(content))),

        // All other non-GET routes are 404s
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("No such resource".into())
            .unwrap()),
    }
}

// see: https://github.com/ctz/hyper-rustls/blob/3f16ac4c36d1133883073b7d6eacf8c09339e87f/examples/server.rs#L122
// Load public certificate from file.
fn load_certs(filename: &str) -> io::Result<Vec<Certificate>> {
    // Open certificate file
    let cert_file = fs::File::open(filename).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to open {}: {}", filename, e),
        )
    })?;

    // Creat reader for the certificate file
    let mut reader = io::BufReader::new(cert_file);

    // Create a vector to store the certificates
    let mut certificates = Vec::new();

    for item in iter::from_fn(|| read_one(&mut reader).transpose()) {
        match item.unwrap() {
            Item::X509Certificate(cert) => certificates.push(Certificate(cert)),
            Item::RSAKey(_) => log::warn!("Unexpected RSAKey in TLS certificate file"),
            Item::PKCS8Key(_) => log::warn!("Unexpected PKCS8Key in TLS certificate file"),
        }
    }

    return Ok(certificates);
}

// https://github.com/ctz/hyper-rustls/blob/3f16ac4c36d1133883073b7d6eacf8c09339e87f/examples/server.rs#L133
// Load private key from file.
fn load_private_key(filename: &str) -> io::Result<PrivateKey> {
    // Open keyfile.
    let key_file = fs::File::open(filename).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to open {}: {}", filename, e),
        )
    })?;

    // Create a reader for the key file
    let mut reader = io::BufReader::new(key_file);
    let mut key: Option<PrivateKey> = None;

    // Use the last private key provided in the file,
    // whether it's RSA or PKCS8
    for item in iter::from_fn(|| read_one(&mut reader).transpose()) {
        match item.unwrap() {
            Item::X509Certificate(_) => log::warn!("Unexpected RSAKey in TLS key file"),
            Item::RSAKey(bytes) => {
                key.replace(PrivateKey(bytes));
            }
            Item::PKCS8Key(bytes) => {
                key.replace(PrivateKey(bytes));
            }
        }
    }

    if let None = key {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to parse a single RSA private key",
        ));
    }

    return Ok(key.unwrap());
}

// https://github.com/ctz/hyper-rustls/blob/3f16ac4c36d1133883073b7d6eacf8c09339e87f/examples/server.rs#L85
struct HyperAcceptor<'a> {
    acceptor: Pin<Box<dyn Stream<Item = Result<TlsStream<TcpStream>, io::Error>> + 'a>>,
}

impl hyper::server::accept::Accept for HyperAcceptor<'_> {
    type Conn = TlsStream<TcpStream>;
    type Error = io::Error;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.acceptor).poll_next(cx)
    }
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Build and initialize env_logger
    let mut filter_builder = env_logger::Builder::new();
    filter_builder.filter(Some("rustls"), LevelFilter::Off);
    filter_builder.parse_env(env_logger::Env::default().filter_or("LOG_LEVEL", "info"));
    filter_builder.init();

    // Parse opts
    let NeoOpts {
        host,
        port,
        stdin_read_timeout_seconds,
        file_path,
        tls_key_path,
        tls_cert_path,
    } = NeoOpts::from_args();

    // Combine host and port into an address, and parse it
    let addr = String::from(format!("{}:{}", host, port)).parse::<SocketAddr>();

    // Stop if parsing failed
    if let Err(_) = addr {
        log::error!("Failed to parse host & port combination");
        process::exit(1);
    }
    let addr = addr.unwrap(); // worry-free unwrap
    log::info!("Server configured to run @ [{}]", addr);

    let mut file_contents = String::new();

    // Attempt to read content from somewhere
    if let Some(path) = file_path {
        // Read from file path
        log::info!("Reading file from path [{}]", path.to_string_lossy());
        file_contents = fs::read_to_string(path)?;
    } else {
        // Read from STDIN
        log::info!(
            "No file path provided, waiting for input on STDIN (max {} seconds)...",
            stdin_read_timeout_seconds,
        );

        let stdin_read_task = tokio::task::spawn_blocking(move || {
            let _ = io::stdin().read_to_string(&mut file_contents);
            return file_contents;
        });

        // Attempt ot read from stdin for a given timeout
        match timeout(
            Duration::from_secs(stdin_read_timeout_seconds),
            stdin_read_task,
        )
        .await
        {
            Ok(Ok(contents)) => {
                file_contents = contents;
                log::info!("Successfully read input from STDIN");
            }
            _ => {
                log::error!(
                    "Failed to read from STDIN after waiting {} seconds",
                    stdin_read_timeout_seconds
                );
                process::exit(1);
            }
        }
    }

    // If contents are *still* empty (no file & STDIN is empty), throw error
    if file_contents.is_empty() {
        log::error!(
            "No file contents -- please ensure you've specified a file or fed in data via STDIN"
        );
        process::exit(1);
    }
    log::info!("Read [{}] characters", file_contents.len());

    // Capture the file contents in an Arc so we can use the reference repeatedly
    // across async tasks that the server will spawn
    let file_contents_bytes = Bytes::from(file_contents);

    // Run the HTTP(S) server
    if tls_key_path.is_none() || tls_cert_path.is_none() {
        // HTTP, if tls key or cert were not supplied
        run_server_http(file_contents_bytes, &addr).await?;
    } else {
        // HTTPS, if both TLS key and cert were supplied
        let tls_key_path = tls_key_path.unwrap();
        let tls_cert_path = tls_cert_path.unwrap();

        run_server_https(file_contents_bytes, &addr, &tls_key_path, &tls_cert_path).await?;
    };

    Ok(())
}

/// Serve HTTPS (TLS)
async fn run_server_https(
    file_contents_bytes: Bytes,
    addr: &SocketAddr,
    tls_key_path: &PathBuf,
    tls_cert_path: &PathBuf,
) -> Result<(), std::io::Error> {
    // Build server
    let svc_builder_fn = make_service_fn(move |_conn| {
        // The move & async combinations that happen in here (including the move above)
        // are a bit complicated.
        // see: https://www.fpcomplete.com/blog/ownership-puzzle-rust-async-hyper/

        // Create a name-shadowed cloned reference to the content we want to serve
        let file_contents = file_contents_bytes.clone();

        async {
            // Create service fn
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                // Create a another name-shadowed, cloned reference
                // since we have moved the original clone past the service_fn boundary
                let file_contents = file_contents.clone();

                serve_static_content(req, file_contents)
            }))
        }
    });

    log::info!(
        "Building TLS configuration with key [{}] and (CA) certs [{}] ...",
        tls_key_path.to_string_lossy(),
        tls_cert_path.to_string_lossy(),
    );
    // Build TLS config
    let tls_cfg = {
        let certs = load_certs(&tls_cert_path.to_string_lossy())?;
        let key = load_private_key(&tls_key_path.to_string_lossy())?;

        // Create server config
        let mut cfg = rustls::ServerConfig::new(
            rustls::AllowAnyAnonymousOrAuthenticatedClient::new(rustls::RootCertStore::empty()),
        );

        // Select a certificate to use
        cfg.set_single_cert(certs, key).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed setting cert for use: {}", e),
            )
        })?;

        // Configure ALPN to accept HTTP/2, HTTP/1.1
        cfg.set_protocols(&[b"h2".to_vec(), b"http/1.1".to_vec()]);

        Arc::new(cfg)
    };

    // Bind to TCP with tokio
    log::info!("Binding TCP on port [{}]...", &addr);
    let tcp = TcpListener::bind(&addr).await?;
    let tls_acceptor = TlsAcceptor::from(tls_cfg);

    // Prepare a long-running future stream handler to accept and serve clients
    let incoming_tls_stream = stream! {
        loop {
            let (socket, _) = tcp.accept().await?;
            let stream = tls_acceptor.accept(socket);

            match stream.await {
                result @ Ok(_) => { yield result; },

                Err(mut err) => {
                    if let Some(inner_err) = err.get_mut() {
                        // TODO: log errors that are *not* bad certificates from new clients
                        if let Some(downcasted) = inner_err.downcast_mut::<TLSError>() {
                            match downcasted {
                                // BadCertificate
                                TLSError::AlertReceived(AlertDescription::BadCertificate) => {
                                    log::debug!("TLS Error (ignored): {}", downcasted);
                                },
                                _ => log::warn!("TLS Error: {}", downcasted),
                            }
                        } else {
                            log::warn!("TLS Error: {}", inner_err);
                        }
                    }
                }
            }
        }
    };

    // Build the serveer object
    let server = Server::builder(HyperAcceptor {
        acceptor: Box::pin(incoming_tls_stream),
    })
    .serve(svc_builder_fn);

    log::info!("Starting HTTPS server...");
    if let Err(e) = server.await {
        log::error!("Server error: {}", &e);
        eprintln!("Server error: {}", e);
    }

    Ok(())
}

/// Serve HTTP
async fn run_server_http(file_contents_bytes: Bytes, addr: &SocketAddr) -> Result<(), io::Error> {
    // Create function that will build the service
    let svc_builder_fn = make_service_fn(move |_conn| {
        // The move & async combinations that happen in here (including the move above)
        // are a bit complicated.
        // see: https://www.fpcomplete.com/blog/ownership-puzzle-rust-async-hyper/

        // Create a name-shadowed cloned reference to the content we want to serve
        let file_contents = file_contents_bytes.clone();

        async {
            // Create service fn
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                // Create a another name-shadowed, cloned reference
                // since we have moved the original clone past the service_fn boundary
                let file_contents = file_contents.clone();

                serve_static_content(req, file_contents)
            }))
        }
    });

    // Regular HTTP server
    let server = Server::bind(&addr).serve(svc_builder_fn);

    log::info!("Starting HTTP server...");
    if let Err(e) = server.await {
        log::error!("Server error: {}", &e);
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
