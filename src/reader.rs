use dsmr5::Readout;
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
pub fn spawn_dsmr_thread(
    rwlock: Arc<RwLock<ReaderData>>,
    path: String,
) -> Result<JoinHandle<()>, std::io::Error> {
    // Open the reader thread and continuously update the rwlock with
    // the DSMR data. If we fail, end the thread and set threadstatus to failed.
    thread::Builder::new().spawn(move || {
        let data = rwlock.clone();

        if let Ok(mut mx) = data.write() {
            mx.thread_status = ThreadStatus::Running;
        }
        debug!("DSMR reader thread spawned.");

        // Initialize reader
        let mut port = serial::open(&path).expect("Failed to set serial port.");
        let _init = match serial_init(&mut port) {
            Ok(res) => info!("Serial port initialized. {:?}", res),
            Err(error) => error!("Failed to initialize serial port: {}", error),
        };
        let mut reader =
            dsmr5::Reader::new(port.bytes().map(|b| b.expect("Failed to map reader.")));

        // The reader is an iterator that yields data
        loop {
            let reader_data = reader.next().expect("No reader data present.");

            match reader_convert_value(reader_data) {
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

            if let Ok(mx) = data.read() {
                if mx.thread_status == ThreadStatus::Stopping {
                    if let Ok(mut mx2) = data.write() {
                        mx2.thread_status = ThreadStatus::Stopped;
                        break;
                    }
                }
            }
        }
    })
}

/// Convert the latest DSMR value to a dsmr state
fn reader_convert_value(
    // port: &mut T,
    data: Readout,
) -> Result<dsmr5::state::State, dsmr5::Error> {
    // Initialize reader
    // let mut reader = dsmr5::Reader::new(port.bytes().map(|b| b.expect("Failed to map reader.")));
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
