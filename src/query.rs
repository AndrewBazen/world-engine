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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_follow_outgoing_edges() {
        let mut graph = ESGraph::new();

        let player = ESNode::new("player", "andrew")
            .with_edge("owns", "item", "sword");
        let item = ESNode::new("item", "sword");

        graph.insert(player.clone());
        graph.insert(item);

        let items = follow(&graph, &player, "owns");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "sword");
        assert_eq!(items[0].node_type, "item");
    }

    #[test]
    fn test_incoming_edges() {
        let mut graph = ESGraph::new();
        let item = ESNode::new("item", "sword")
            .with_edge("owned_by", "player", "andrew");
        graph.insert(item);
        let owned_by = incoming(&graph, "player", "andrew", "owned_by");
        assert_eq!(owned_by.len(), 1);
        assert_eq!(owned_by[0].id, "sword");
    }
}
