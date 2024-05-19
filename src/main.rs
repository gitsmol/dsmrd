use crate::{
    endpoints::handler,
    reader::{spawn_dsmr_thread, ReaderData},
};
use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use log::{debug, error, info, warn};
use std::{
    convert::Infallible,
    env,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
};

mod endpoints;
mod reader;

#[tokio::main]
async fn main() {
    env_logger::init();
    let path = match env::args().nth(2) {
        Some(path) => path.to_owned(),
        None => String::from("/dev/ttyUSB0"),
    };
    info!("Using DSMR-reader at {:?}", path);

    // Create a mutex inside an Arc to store the DSMR state.
    let dsmr_state = Arc::new(Mutex::new(ReaderData::default()));

    // Spawn the thread containing the DSMR reader. This continuously retrieves
    // data from the reader and stores it in the mutex.
    match spawn_dsmr_thread(dsmr_state.clone(), path) {
        Ok(_) => debug!("Spawned DSMR thread."),
        Err(e) => warn!("Error spawning DSMR thread: {}", e),
    }

    // A `MakeService` that produces a `Service` to handle each connection.
    let get_last_value = make_service_fn(move |_| {
        // Clone mutex to share it with each invocation of `make_service`.
        let dsmr_state = dsmr_state.clone();

        // Create a `Service` for responding to the request.
        // Note: this is yet another context so we clone the mutex again!
        let service = service_fn(move |req| handler(req, dsmr_state.clone()));

        // Return the service to hyper.
        async move { Ok::<_, Infallible>(service) }
    });

    // We'll bind to 127.0.0.1:3000 unless we find an ip in the env args
    let mut addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    if let Some(given_addr) = env::args().nth(1) {
        if let Ok(parsed_addr) = SocketAddr::from_str(&given_addr) {
            debug!("Assigning {} to server.", parsed_addr);
            addr = parsed_addr
        };
    };

    let server = Server::bind(&addr).serve(get_last_value);

    info!("Listening on http://{}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        error!("server error: {}", e);
    }
}
