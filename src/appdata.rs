use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    usize,
};

use event_listener::{Event, EventListener};

#[derive(Clone, Debug)]
pub struct AppData {
    local_addr: SocketAddr,
    pub client_register: Arc<RwLock<Vec<SocketAddr>>>,
    event_listener: Arc<Event>,
}

impl AppData {
    pub fn new(local_addr: SocketAddr) -> Self {
        Self {
            local_addr,
            client_register: Arc::new(RwLock::new(Vec::new())),
            event_listener: Arc::new(Event::new()),
        }
    }

    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    pub fn emit_event(&self) {
        self.event_listener.notify(usize::MAX);
    }

    pub fn event_listener(&self) -> EventListener {
        self.event_listener.listen()
    }

    pub fn register_client(&self, client_addr: SocketAddr) -> Result<(), String> {
        if let Ok(mut register) = self.client_register.write() {
            if register.contains(&client_addr) {
                return Err(String::from("Client already registered!"));
            };
            register.push(client_addr);
            Ok(())
        } else {
            Err(String::from("Unable to register client!"))
        }
    }

    pub fn unregister_client(&self, client_addr: SocketAddr) -> Result<(), String> {
        if let Ok(mut register) = self.client_register.write() {
            register.retain(|&addr| addr != client_addr);
            Ok(())
        } else {
            Err(String::from("Unable to unregister client!"))
        }
    }

    pub fn list_clients(&self) -> Result<Vec<String>, String> {
        match self.client_register.read() {
            Ok(register) => {
                let result: Vec<String> = register.iter().map(|f| f.to_string()).collect();
                Ok(result)
            }
            Err(e) => Err(format!("Error reading register: {}", e)),
        }
    }
}
