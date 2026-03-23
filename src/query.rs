use crate::graph::{ESGraph, ESNode};

// follow outgoing edges from a node by label
// e.g. follow(&graph, player_node, "owns") -> all items the player owns
pub fn follow<'a>(graph: &'a ESGraph, node: &ESNode, label: &str) -> Vec<&'a ESNode> {
    node.edges_by_label(label)
        .iter()
        .filter_map(|e| graph.get(&e.target_type, &e.target_id))
        .collect()
}

// find all nodes that have an edge pointing to a target
// e.g. incoming(&graph: "player", "andrew", "owned_by") -> items owned by andrew
pub fn incoming<'a>(graph: &'a ESGraph, target_type: &str, target_id: &str, label: &str) -> Vec<&'a ESNode> {
    graph.nodes.values()
        .filter(|node| node.has_edge(label, target_type, target_id))
        .collect()
}