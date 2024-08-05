use crate::{
    endpoints::handler,
    reader::{spawn_dsmr_thread, ReaderData},
};
use appdata::AppData;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use log::{debug, error, info};
use std::{
    convert::Infallible,
    env,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, RwLock},
};
use udp_sender::spawn_udp_sender;

mod appdata;
mod endpoints;
mod reader;
mod udp_sender;

#[tokio::main]
async fn main() {
    env_logger::init();
    let path = match env::args().nth(2) {
        Some(path) => path.to_owned(),
        None => String::from("/dev/ttyUSB0"),
    };
    info!("Using DSMR-reader at {:?}", path);

    // Create a mutex inside an Arc to store the DSMR state.
    let dsmr_state = Arc::new(RwLock::new(ReaderData::default()));

    // We'll bind to 127.0.0.1:3000 unless we find an ip in the env args
    let mut addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    if let Some(given_addr) = env::args().nth(1) {
        if let Ok(parsed_addr) = SocketAddr::from_str(&given_addr) {
            debug!("Assigning {} to server.", parsed_addr);
            addr = parsed_addr
        };
    };

    let appdata = Arc::new(AppData::new(addr));

    // Spawn the thread running the DSMR reader. This continuously retrieves
    // data from the reader and stores it in an rwlock. Emits an event when new data is
    // stored.
    match spawn_dsmr_thread(appdata.clone(), dsmr_state.clone(), path) {
        Ok(_) => debug!("Spawned DSMR thread."),
        Err(e) => panic!("Error spawning DSMR thread: {}", e),
    }

    // Spawn the thread running the UDP sender. This continuously checks for new data by
    // listening to the event in appdata.
    match spawn_udp_sender(appdata.clone(), dsmr_state.clone()) {
        Ok(_) => debug!("Spawned UDP sender thread."),
        Err(e) => panic!("Error spawning UDP sender thread"),
    };

    let dsmr_service = make_service_fn(move |_con: &AddrStream| {
        // Clone mutex to share it with each invocation of `make_service`.
        let dsmr_state = dsmr_state.clone();
        let appdata = appdata.clone();

        // Create a `Service` for responding to the request.
        // Note: this is yet another context so we clone the mutex again!
        let service = service_fn(move |req| handler(req, dsmr_state.clone(), appdata.clone()));

        // Return the service to hyper.
        async move { Ok::<_, Infallible>(service) }
    });

    let server = Server::bind(&addr).serve(dsmr_service);

    info!("Listening on http://{}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        error!("server error: {}", e);
    }
}
