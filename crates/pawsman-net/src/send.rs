//! One request over one connection: DNS, TCP, optional TLS, HTTP/1.1.

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::body::Collector;
use crate::runtime::NetConfig;
use crate::{NetError, Outgoing, Phase, Response};

/// Sends `outgoing` and reads the whole response.
pub(crate) async fn perform(
    outgoing: &Outgoing,
    config: NetConfig,
    tls: Arc<rustls::ClientConfig>,
) -> Result<Response, NetError> {
    let started = Instant::now();
    let uri = outgoing.uri();
    let host = outgoing.host().to_owned();
    let https = uri.scheme_str() == Some("https");
    let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
    let connecting = async {
        let tcp = connect(&host, port).await?;
        if https {
            let name = ServerName::try_from(host.clone())
                .map_err(|_| NetError::tls(&host, "not a valid server name"))?;
            let stream = tokio_rustls::TlsConnector::from(tls)
                .connect(name, tcp)
                .await
                .map_err(|e| NetError::tls(&host, &e.to_string()))?;
            Ok(Io::Tls(Box::new(stream)))
        } else {
            Ok(Io::Plain(tcp))
        }
    };
    let io = tokio::time::timeout(config.connect_timeout_value(), connecting)
        .await
        .map_err(|_| NetError::timeout(Phase::Connect))??;
    match io {
        Io::Plain(stream) => exchange(stream, outgoing, config, started).await,
        Io::Tls(stream) => exchange(*stream, outgoing, config, started).await,
    }
}

enum Io {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

async fn connect(host: &str, port: u16) -> Result<TcpStream, NetError> {
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| NetError::dns(host, &e))?;
    let target = format!("{host}:{port}");
    let mut last = None;
    for addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(err) => last = Some(err),
        }
    }
    let err = last.unwrap_or_else(|| std::io::Error::other("no address"));
    Err(NetError::connect(&target, &err))
}

async fn exchange<S>(
    stream: S,
    outgoing: &Outgoing,
    config: NetConfig,
    started: Instant,
) -> Result<Response, NetError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let protocol = |e: hyper::Error| NetError::protocol(&e.to_string());
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(protocol)?;
    let connection = tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = build(outgoing)?;
    let response = sender.send_request(request).await.map_err(protocol);
    let response = match response {
        Ok(response) => response,
        Err(err) => {
            connection.abort();
            return Err(err);
        }
    };
    let ttfb = started.elapsed();
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let mut body = response.into_body();
    let mut collector = Collector::new(config.body_cap_value());
    let collected = async {
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(protocol)?;
            if let Some(data) = frame.data_ref() {
                collector.push(data).await?;
            }
        }
        Ok::<_, NetError>(())
    }
    .await;
    connection.abort();
    collected?;
    Ok(Response {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_owned(),
        headers,
        body: collector.finish().await?,
        ttfb,
        elapsed: started.elapsed(),
    })
}

fn build(outgoing: &Outgoing) -> Result<http::Request<Full<Bytes>>, NetError> {
    let uri = outgoing.uri();
    let path = uri
        .path_and_query()
        .map_or("/", http::uri::PathAndQuery::as_str);
    let host = uri.host().unwrap_or("");
    let host = match uri.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    let headers = outgoing.headers();
    let mut request = http::Request::builder()
        .method(outgoing.method().clone())
        .uri(path);
    if !headers.contains_key(http::header::HOST) {
        request = request.header(http::header::HOST, host);
    }
    // hyper sends a bodiless POST with no length at all; strict servers answer 411.
    let wants_length = matches!(
        *outgoing.method(),
        http::Method::POST | http::Method::PUT | http::Method::PATCH
    );
    if wants_length
        && outgoing.body().is_empty()
        && !headers.contains_key(http::header::CONTENT_LENGTH)
        && !headers.contains_key(http::header::TRANSFER_ENCODING)
    {
        request = request.header(http::header::CONTENT_LENGTH, "0");
    }
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request
        .body(Full::new(outgoing.body().clone()))
        .map_err(|e| NetError::invalid_request("request", &e.to_string()))
}
