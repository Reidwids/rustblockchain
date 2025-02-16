use crate::ownership::node::get_node_id;

pub fn handle_get_node_id() {
    let node_id = get_node_id();
    println!("Node ID: {}", node_id);
}
