use log::{debug, error, info};
use serde::Serialize;
use serial::prelude::*;
use std::io::Read;

use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(PartialEq, Eq, Debug, Serialize)]
pub enum ThreadStatus {
    Running,
    Failed,
    Stopping,
    Stopped,
}

pub struct ReaderData {
    pub dsmr_state: dsmr5::state::State,
    pub thread_status: ThreadStatus,
    pub thread_handle: Option<JoinHandle<()>>,
}

impl Default for ReaderData {
    fn default() -> Self {
        Self {
            dsmr_state: dsmr5::state::State::default(),
            thread_status: ThreadStatus::Stopped,
            thread_handle: None,
        }
    }
}

/// Spawn a thread that endlessly reads the DSMR and stores its state in rwlock.
/// Returns Ok() and adds thread handle to rwlock; returns Err.
pub fn spawn_dsmr_thread(
    rwlock: Arc<RwLock<ReaderData>>,
    path: String,
) -> Result<(), std::io::Error> {
    let data = rwlock.clone();

    // Open the reader thread and continuously update the rwlock with
    // the DSMR data. If we fail, end the thread and set threadstatus to failed.
    let thread = thread::Builder::new().spawn(move || {
        let data = rwlock.clone();

        debug!("DSMR reader thread spawned.");
        let mut port = serial::open(&path).expect("Failed to set serial port.");
        let _init = match serial_init(&mut port) {
            Ok(res) => info!("Serial port initialized. {:?}", res),
            Err(error) => error!("Failed to initialize serial port: {}", error),
        };
        loop {
            match reader_get_value(&mut port) {
                Ok(state) => {
                    debug!("DSMR reader value received.");
                    if let Ok(mut mx) = data.write() {
                        mx.dsmr_state = state;
                    }
                }
                Err(e) => {
                    debug!("Unable to receive DSMR reader value: {:?}", e);
                    if let Ok(mut mx) = data.write() {
                        mx.thread_status = ThreadStatus::Failed;
                        break;
                    }
                }
            };
        }
    });

    match thread {
        Ok(handle) => {
            if let Ok(mut mx) = data.write() {
                mx.thread_handle = Some(handle);
            };
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Get the latest DSMR value
fn reader_get_value<T: serial::SerialPort>(
    port: &mut T,
) -> Result<dsmr5::state::State, dsmr5::Error> {
    // Initialize reader
    let mut reader = dsmr5::Reader::new(port.bytes().map(|b| b.expect("Failed to map reader.")));
    let data = reader.next().expect("No reader data present.");
    let data = match data.to_telegram() {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to get data from reader");
            return Err(e);
        }
    };
    let state = match dsmr5::Result::<dsmr5::state::State>::from(&data) {
        Ok(state) => state,
        Err(e) => {
            error!("Failed to process state");
            return Err(e);
        }
    };
    Ok(state)
}

/// Initialize the serial connection to the DSMR
fn serial_init<T: SerialPort>(port: &mut T) -> serial::Result<()> {
    port.reconfigure(&|settings| {
        settings
            .set_baud_rate(serial::Baud115200)
            .expect("Failed to set baud rate.");
        settings.set_char_size(serial::Bits8);
        settings.set_parity(serial::ParityNone);
        settings.set_stop_bits(serial::Stop1);
        settings.set_flow_control(serial::FlowNone);
        Ok(())
    })
    .expect("Failed to set configuration.");

    port.set_timeout(Duration::from_millis(1000))
        .expect("Failed to set timeout.");

    let mut buf: Vec<u8> = (0..255).collect();

    port.write(&buf[..]).expect("Port write failed.");
    port.read(&mut buf[..]).expect("Port read failed.");

    Ok(())
}
