use log::{error, info, warn};
use serial::prelude::*;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::Duration;

struct ReaderData {
    reader_state: ReaderState,
    dsmr_state: dsmr5::state::State,
    thread: Thread,
}

enum ReaderState {
    Running,
    Stopped,
    Failed,
}

/// Spawn a thread that endlessly reads the DSMR and stores its state into a mutex.
pub async fn spawn_dsmr_thread(mutex: Arc<Mutex<dsmr5::state::State>>) {
    thread::spawn(move || {
        let path = "/dev/DSMR-reader";
        let mut port = serial::open(&path).expect("Failed to set serial port.");
        let _init = match serial_init(&mut port) {
            Ok(_init) => info!("Serial port initialized."),
            Err(error) => error!("Failed to initialize serial port: {}", error),
        };
        loop {
            if let Ok(state) = reader_get_value(&mut port) {
                let mut mx = mutex.lock().unwrap();
                *mx = state;
            };
        }
    });
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
