use event_listener::Listener;

use std::{
    net::UdpSocket,
    sync::{Arc, RwLock},
    thread::{self, JoinHandle},
};

use log::debug;

use crate::{appdata::AppData, reader::ReaderData};

/// Spawns a thread that sends new dsmr_data to registered clients using UDP packets.
/// Waits for an EventListener to signal new data, then reads data from the RwLock and
/// sends it to all registered clients.
pub fn spawn_udp_sender(
    appdata: Arc<AppData>,
    reader_data: Arc<RwLock<ReaderData>>,
) -> Result<JoinHandle<()>, std::io::Error> {
    thread::Builder::new().spawn(move || {
        let mut socket_addr = *appdata.local_addr();
        socket_addr.set_port(0); // Set port to 0 to let the OS assign a random free port
        let sock = UdpSocket::bind(socket_addr).expect("Failed to bind UDP socket");
        let assigned_port = sock
            .local_addr()
            .expect("Failed to get local address")
            .port();
        println!("UDP service started on port: {}", assigned_port);

        // inner loop
        loop {
            let listener = appdata.event_listener();
            listener.wait();

            debug!("Received data");
            let Ok(dsmr_data) = reader_data.read() else {
                continue;
            };
            let Ok(addresses) = appdata.client_register.as_ref().read() else {
                continue;
            };

            if let Ok(ser_data) = serde_json::to_vec(&dsmr_data.dsmr_state) {
                for addr in addresses.iter() {
                    if let Ok(length) = sock.send_to(&ser_data, addr) {
                        debug!("Sent {} bytes to {}", length, addr)
                    };
                }
            }
        }
    })
}
