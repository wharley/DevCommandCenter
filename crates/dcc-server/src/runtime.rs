//! Background work on the caller's Tokio runtime, or on a small one of the
//! server's own when there is none (setup hooks, plain threads, tests).

use std::future::Future;
use std::sync::OnceLock;

pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
        }
        Err(_) => {
            fallback().spawn(future);
        }
    }
}

fn fallback() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("dcc-server")
            .enable_all()
            .build()
            .expect("the server's background runtime")
    })
}
