use crate::{
    appdata::AppData,
    reader::{spawn_dsmr_thread, ReaderData, ThreadStatus},
};
use hyper::{Body, Request, Response, StatusCode};
use log::debug;
use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

/// Handler for all incoming http requests
pub async fn handler(
    req: Request<Body>,
    data: Arc<RwLock<ReaderData>>,
    appdata: Arc<AppData>,
) -> Result<Response<Body>, hyper::http::Error> {
    debug!("Received request: {:?}", req);
    match req.uri().to_string() {
        u if u.starts_with("/status") => get_latest_data(data).await,
        u if u.starts_with("/start") => start_thread(appdata, data).await,
        u if u.starts_with("/stop") => stop_thread(data).await,
        u if u.starts_with("/register") => register_client(appdata, req).await,
        u if u.starts_with("/unregister") => unregister_client(appdata, req).await,
        u if u.starts_with("/list") => list_clients(appdata).await,
        _ => get_state(data).await,
    }
}

async fn get_state(data: Arc<RwLock<ReaderData>>) -> Result<Response<Body>, hyper::http::Error> {
    // Get a lock on the mutex containing the DSMR data
    let content = data.read().expect("Failed to read RwLock...");
    // Deserialize the data to a json string.
    let json = serde_json::to_string(&content.dsmr_state);

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

async fn get_latest_data(
    mutex: Arc<RwLock<ReaderData>>,
) -> Result<Response<Body>, hyper::http::Error> {
    let data = mutex.read().expect("Failed to read RwLock...");
    let json = serde_json::to_string(&data.thread_status);
    if let Ok(json) = json {
        // If we can get a json string, return that.
        Ok(Response::new(Body::from(json)))
    } else {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Failed to retrieve DSMR data."))
    }
}

async fn start_thread(
    appdata: Arc<AppData>,
    rwlock: Arc<RwLock<ReaderData>>,
) -> Result<Response<Body>, hyper::http::Error> {
    // Check if we already have a running thread.
    // Do this in a separate scope so the mutex gets unlocked/released after.
    {
        if rwlock
            .read()
            .expect("Failed to read RwLock...")
            .thread_handle
            .is_some()
        {
            debug!("Found existing thread. Not creating new thread.");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Error: existing DMSR reader thread found."));
        };
    }

    // Spawn the dsmr thread and return a response.
    match spawn_dsmr_thread(appdata, rwlock, String::from("/dev/ttyUSB0")) {
        Ok(_) =>
        // Return Ok statuscode.
        {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("New DSMR reader thread started."))
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!(
                "Error: failed to start DSMR reader thread.\n{}",
                e,
            ))),
    }
}

async fn stop_thread(
    rwlock: Arc<RwLock<ReaderData>>,
) -> Result<Response<Body>, hyper::http::Error> {
    let ok_response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("DMSR reader thread stopped."));

    // Get a lock on the mutex containing the DSMR data
    let mut data = rwlock.write().expect("Unable to write to RwLock...");
    data.thread_status = ThreadStatus::Stopping;

    ok_response
}

async fn list_clients(appdata: Arc<AppData>) -> Result<Response<Body>, hyper::http::Error> {
    match appdata.list_clients() {
        Ok(res) =>
        // Return Ok statuscode.
        {
            let mut response_string = "Currently registered clients are\n".to_string();
            response_string.extend(res);
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(response_string))
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!(
                "Error: failed to provide client register. {}",
                e
            ))),
    }
}

async fn register_client(
    appdata: Arc<AppData>,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::http::Error> {
    let Ok(remote_addr) = parse_client_addr(req).await else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("Error: failed to register client.",)));
    };
    debug!("Registering client {}", remote_addr);

    match appdata.register_client(remote_addr) {
        Ok(_) =>
        // Return Ok statuscode.
        {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(format!(
                    "Succesfully registered client {}",
                    remote_addr
                )))
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!(
                "Error: failed to register client {}: {}",
                remote_addr, e,
            ))),
    }
}

async fn unregister_client(
    appdata: Arc<AppData>,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::http::Error> {
    let Ok(remote_addr) = parse_client_addr(req).await else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("Error: failed to unregister client.",)));
    };
    debug!("Unregistering client {}", remote_addr);

    match appdata.unregister_client(remote_addr) {
        Ok(_) =>
        // Return Ok statuscode.
        {
            Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(format!(
                    "Succesfully unregistered client {}",
                    remote_addr
                )))
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!(
                "Error: failed to unregister client {}: {}",
                remote_addr, e,
            ))),
    }
}

/// Helper function to parse requests to (un)register into SocketAddr using ip and port.
async fn parse_client_addr(req: Request<Body>) -> Result<SocketAddr, Box<dyn Error>> {
    let query = req.uri().query().ok_or("No query string found")?;

    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

    let ip = params.get("ip").ok_or("No IP parameter found")?;
    let port = params.get("port").ok_or("No port parameter found")?;

    let addr = format!("{}:{}", ip, port);
    let socket_addr: SocketAddr = addr.parse()?;

    Ok(socket_addr)
}
