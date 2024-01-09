/// Example on how to use the Hyper server in !Send mode.
/// The clients are harder, see https://github.com/hyperium/hyper/issues/2341 for details
///
/// Essentially what we do is we wrap our types around the Tokio traits. The
/// `!Send` limitation makes it harder to deal with high level hyper primitives,
/// but it works in the end.
mod hyper_compat {
    use futures_lite::{AsyncRead, AsyncWrite, Future};
    use hyper::service::service_fn;
    use std::{
        net::SocketAddr,
        pin::Pin,
        task::{Context, Poll},
    };

    use glommio::{
        enclose,
        net::{TcpListener, TcpStream},
        sync::Semaphore,
    };
    use hyper::{server::conn::http2, server::conn::http1, body::Incoming, Request, Response};
    use std::{io, rc::Rc};
    use hyper::rt::ReadBufCursor;
    use http_body_util::{BodyExt, Full};
    use bytes::{Buf, Bytes};

    type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

    #[derive(Clone)]
    struct HyperExecutor;

    impl<F> hyper::rt::Executor<F> for HyperExecutor
        where
            F: Future + 'static,
            F::Output: 'static,
    {
        fn execute(&self, fut: F) {
            glommio::spawn_local(fut).detach();
        }
    }

    struct HyperStream(pub TcpStream);

    impl hyper::rt::Read for HyperStream {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            mut buf: ReadBufCursor<'_>,
        ) -> Poll<io::Result<()>> {
            let HyperStream(ref mut stream) = self.get_mut();

            // SAFETY: We will initialize this buffer before using it.
            let buffer = unsafe {
                let unfilled = buf.as_mut();
                for byte in unfilled.iter_mut() {
                    byte.as_mut_ptr().write(0);
                }
                std::slice::from_raw_parts_mut(unfilled.as_mut_ptr() as *mut u8, unfilled.len())
            };

            // Poll the underlying stream
            match Pin::new(stream).poll_read(cx, buffer) {
                Poll::Ready(Ok(n)) => {
                    // Advance the cursor by the number of bytes read
                    unsafe { buf.advance(n); }
                    Poll::Ready(Ok(()))
                },
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl hyper::rt::Write for HyperStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.0).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
            Pin::new(&mut self.0).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
            Pin::new(&mut self.0).poll_close(cx)
        }
    }

    pub(crate) async fn serve_http<S, F, R, A>(
        addr: A,
        service: S,
        max_connections: usize,
    ) -> io::Result<()>
        where
            S: Fn(Request<Incoming>) -> F + 'static + Copy,
            F: Future<Output = Result<Response<BoxBody>, R>> + 'static,
            R: std::error::Error + 'static + Send + Sync,
            A: Into<SocketAddr>,
    {
        let listener = TcpListener::bind(addr.into())?;
        let conn_control = Rc::new(Semaphore::new(max_connections as _));
        loop {
            match listener.accept().await {
                Err(x) => {
                    return Err(x.into());
                }
                Ok(stream) => {
                    let addr = stream.local_addr().unwrap();
                    glommio::spawn_local(enclose!{(conn_control) async move {
                        let _permit = conn_control.acquire_permit(1).await;
                        if let Err(x) = http1::Builder::new(HyperExecutor).serve_connection(HyperStream(stream), service_fn(service)).await {
                            if !x.is_incomplete_message() {
                                eprintln!("Stream from {addr:?} failed with error {x:?}");
                            }
                        }
                    }}).detach();
                }
            }
        }
    }
}

use glommio::{CpuSet, LocalExecutorPoolBuilder, PoolPlacement};
use hyper::{body::Incoming, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use std::convert::Infallible;
use bytes::{Buf, Bytes};

type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
async fn hyper_demo(req: Request<Incoming>) -> Result<Response<BoxBody>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/hello") => Ok(Response::new(full("world"))),
        (&Method::GET, "/world") => Ok(Response::new(full("hello"))),
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(full("notfound"))
            .unwrap()),
    }
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

fn main() {
    // Issue curl -X GET http://127.0.0.1:8000/hello or curl -X GET http://127.0.0.1:8000/world to
    // see it in action

    println!("Starting server on port 8000");

    LocalExecutorPoolBuilder::new(PoolPlacement::MaxSpread(
        num_cpus::get(),
        CpuSet::online().ok(),
    ))
        .on_all_shards(|| async move {
            let id = glommio::executor().id();
            println!("Starting executor {id}");
            hyper_compat::serve_http(([0, 0, 0, 0], 8000), hyper_demo, 1024)
                .await
                .unwrap();
        })
        .unwrap()
        .join_all();
}