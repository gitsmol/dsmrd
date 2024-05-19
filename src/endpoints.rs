use crate::reader::{spawn_dsmr_thread, ReaderData, ThreadStatus};
use hyper::{Body, Request, Response, StatusCode};
use log::debug;
use std::sync::{Arc, Mutex};

/// Handler that returns the currently stored DSMR frame as a HTTP result.
pub async fn handler(
    req: Request<Body>,
    mutex: Arc<Mutex<ReaderData>>,
) -> Result<Response<Body>, hyper::http::Error> {
    debug!("Received request: {:?}", req);
    match req.uri().to_string() {
        u if u.starts_with("/status") => check_thread(mutex).await,
        u if u.starts_with("/start") => start_thread(mutex).await,
        u if u.starts_with("/stop") => stop_thread(mutex).await,
        _ => get_state(mutex).await,
    }
}

async fn get_state(mutex: Arc<Mutex<ReaderData>>) -> Result<Response<Body>, hyper::http::Error> {
    // Get a lock on the mutex containing the DSMR data
    let data = mutex.lock().unwrap();
    // Deserialize the data to a json string.
    let json = serde_json::to_string(&data.dsmr_state);

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

async fn check_thread(mutex: Arc<Mutex<ReaderData>>) -> Result<Response<Body>, hyper::http::Error> {
    let data = mutex.lock().unwrap();
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

async fn start_thread(mutex: Arc<Mutex<ReaderData>>) -> Result<Response<Body>, hyper::http::Error> {
    // Check if we already have a running thread.
    // Do this in a separate scope so the mutex gets unlocked/released after.
    {
        let data = mutex.clone();
        let data = data.lock().unwrap();
        if data.thread_status == ThreadStatus::Running {
            debug!("Found existing thread. Not creating new thread.");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Error: existing DMSR reader thread found."));
        };
    }

    // Spawn the dsmr thread and return a response.
    match spawn_dsmr_thread(mutex, String::from("/dev/ttyUSB0")) {
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

async fn stop_thread(mutex: Arc<Mutex<ReaderData>>) -> Result<Response<Body>, hyper::http::Error> {
    let error_response = Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from("Failed to stop DSMR thread."));

    let ok_response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("DMSR reader thread stopped."));

    // Get a lock on the mutex containing the DSMR data
    if let Ok(mut data) = mutex.lock() {
        if data.thread_status == ThreadStatus::Running || data.thread_status == ThreadStatus::Failed
        {
            // Give the signal to the thread to stop.
            data.thread_status = ThreadStatus::Stopping;
            // Return Ok statuscode.
            return ok_response;
        }
    }

    // Failover
    error_response
}
