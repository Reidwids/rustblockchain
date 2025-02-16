use std::{
    fs::OpenOptions,
    io::{Read, Write},
};

use uuid::Uuid;

const NODE_PATH: &str = "./data/node.data";

/// Get the local node ID. Saves locally, and will a new instance if one does not already exist
pub fn get_node_id() -> Uuid {
    // Get or create local node ID store
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(NODE_PATH)
        .expect("[node::get_node_id] ERROR: Failed to get or create file");

    // Read file
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .expect("[node::get_node_id] ERROR: Failed to read file");

    // If file is not empty, return the UUID
    if let Ok(uuid) = Uuid::parse_str(buf.trim()) {
        return uuid;
    }

    // Else create the Uuid
    let new_uuid = Uuid::new_v4();
    file.set_len(0)
        .expect("[node::get_node_id] ERROR: Failed to clear file");
    file.write_all(new_uuid.to_string().as_bytes())
        .expect("[node::get_node_id] ERROR: Failed to write node uuid to file");
    new_uuid
}
