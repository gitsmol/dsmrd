use std::{
    convert::Infallible,
    env,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use log::{debug, error, info, warn};

use crate::reader::spawn_dsmr_thread;

pub mod reader;

/// Handler that returns the currently stored DSMR frame as a HTTP result.
async fn handler(
    _req: Request<Body>,
    mutex: Arc<Mutex<dsmr5::state::State>>,
) -> Result<Response<Body>, hyper::http::Error> {
    // Get a lock on the mutex containing the DSMR data
    let data = mutex.lock().unwrap();
    // Deserialize the data to a json string.
    let json = serde_json::to_string(&*data);

    if let Ok(json) = json {
        // If we can get a json string, return that.
        // Note: this should always succeed because worst case
        // the DSMR state returns a 'null-frame' containing no data
        // or the last frame that was succesfully stored
        // It is up to the client to make sure the data is useful/valid.
        Ok(Response::new(Body::from(json)))
    } else {
        // If not, return a HTTP error.
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Failed to retrieve DSMR data."))
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    // Create a mutex inside an Arc to store the DSMR state.
    let dsmr_state = Arc::new(Mutex::new(dsmr5::state::State::default()));

    // Spawn the thread containing the DSMR reader. This continuously retrieves
    // data from the reader and stores it in the mutex.
    spawn_dsmr_thread(dsmr_state.clone()).await;
    debug!("Spawned DSMR thread.");

    // A `MakeService` that produces a `Service` to handle each connection.
    let make_service = make_service_fn(move |_| {
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

    let server = Server::bind(&addr).serve(make_service);

    info!("Listening on http://{}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        error!("server error: {}", e);
    }
}
