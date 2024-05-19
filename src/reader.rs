use log::{debug, error, info};
use serde::Serialize;
use serial::prelude::*;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
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
    pub thread_stop_tx: Sender<bool>,
    pub thread_stop_rx: Receiver<bool>,
}

impl Default for ReaderData {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            dsmr_state: dsmr5::state::State::default(),
            thread_status: ThreadStatus::Stopped,
            thread_stop_tx: tx,
            thread_stop_rx: rx,
        }
    }
}

/// Spawn a thread that endlessly reads the DSMR and stores its state into a mutex.
pub fn spawn_dsmr_thread(
    mutex: Arc<Mutex<ReaderData>>,
    path: String,
) -> Result<(), std::io::Error> {
    let reader_data = mutex.clone();

    let thread = thread::Builder::new().spawn(move || {
        debug!("DSMR reader thread spawned.");
        // let path = "/dev/DSMR-reader";
        let mut port = serial::open(&path).expect("Failed to set serial port.");
        let _init = match serial_init(&mut port) {
            Ok(res) => info!("Serial port initialized. {:?}", res),
            Err(error) => error!("Failed to initialize serial port: {}", error),
        };
        loop {
            match reader_get_value(&mut port) {
                Ok(state) => {
                    debug!("DSMR reader value received.");
                    let mut mx = reader_data.lock().unwrap();
                    mx.dsmr_state = state;
                    debug!("DSMR thread status: {:?}", &mx.thread_status);
                    if mx.thread_status == ThreadStatus::Stopping {
                        debug!("Stopping thread with status: {:?}", &mx.thread_status);
                        mx.thread_status = ThreadStatus::Stopped;
                        break;
                    }
                }
                Err(_e) => {
                    debug!("Unable to receive DSMR reader value.");
                    let mut mx = reader_data.lock().unwrap();
                    mx.thread_status = ThreadStatus::Failed;
                    break;
                }
            };
        }
    });

    // If the thread started okay, set thread status to Running.
    match thread {
        Ok(_) => {
            // Set thread status to running.
            let mut mx = mutex.lock().unwrap();
            mx.thread_status = ThreadStatus::Running;
            debug!("Thread status set to running.");
            Ok(())
        }
        Err(e) => {
            // If thread start failed, set status to failed.
            let mut mx = mutex.lock().unwrap();
            mx.thread_status = ThreadStatus::Failed;
            debug!("Thread status set to failed.");
            Err(e)
        }
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
