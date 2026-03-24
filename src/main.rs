mod graph;
mod query;
mod parser;
mod serializer;

use graph::{ESGraph, ESNode, ESValue};
use query::{follow, incoming};
use parser::parse;
use serializer::serialize;


fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_node_creation() {
        let mut graph = ESGraph::new();
        
        let player = ESNode::new("player", "andrew")
            .with_prop("strength", ESValue::Number(14.0))
            .with_prop("class", ESValue::Text("Compensated Anarchist".to_string()))
            .with_prop("alive", ESValue::Bool(true))
            .with_prop("intelligence", ESValue::Number(14.0))
            .with_edge("playing", "session", "s1");

        graph.insert(player);
        
        // can we get it back?
        let retrieved = graph.get("player", "andrew");
        assert!(retrieved.is_some());

        let node = retrieved.unwrap();
        assert_eq!(node.id, "andrew");
        assert_eq!(node.node_type, "player");

        // check prop types
        let strength = node.props.get("strength");
        let class = node.props.get("class");
        let alive = node.props.get("alive");
        assert!(matches!(strength, Some(ESValue::Number(v)) if *v == 14.0));
        assert!(matches!(class, Some(ESValue::Text(s)) if s == "Compensated Anarchist"));
        assert!(matches!(alive, Some(ESValue::Bool(b)) if *b == true));
        

        // check an edge
        assert_eq!(node.edges.len(), 1);
        assert_eq!(node.edges[0].label, "playing");
        assert_eq!(node.edges[0].target_type, "session");
        assert_eq!(node.edges[0].target_id, "s1");
    }

    #[test]
    fn test_missing_node_returns_none() {
        let graph = ESGraph::new();
        assert!(graph.get("player", "nobody").is_none());
    }

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

    #[test]
    fn test_parse_node_declaration() {
        let input = "@player:andrew";

        let graph = parse(input);
        
        assert!(graph.nodes.contains_key("player:andrew"));
        let retrieved = graph.get("player", "andrew");
        assert!(retrieved.is_some());

        let node = retrieved.unwrap();
        assert_eq!(node.node_type, "player");
        assert_eq!(node.id, "andrew");
    }

    #[test]
    fn test_parse_property() {
        let input = "
          @player:andrew
            class: \"Compensated Anarchist\"
            strength: 12.0
            alive: true
            ";

        let graph = parse(input);

        let retrieved = graph.get("player", "andrew");
        assert!(retrieved.is_some());

        let node = retrieved.unwrap();
        assert_eq!(node.props.len(), 3);
        assert!(matches!(node.props.get("class"), Some(ESValue::Text(s)) if s == "Compensated Anarchist"));
        assert!(matches!(node.props.get("strength"), Some(ESValue::Number(v)) if *v == 12.0));
        assert!(matches!(node.props.get("alive"), Some(ESValue::Bool(b)) if *b == true));
    }

    #[test]
    fn test_parse_edge() {
        let input = "
    @player:andrew
    --[owns]--> @item:sword

    @item:sword
    ";
        let graph = parse(input);
        let player = graph.get("player", "andrew").unwrap();
        assert_eq!(player.edges.len(), 1);
        assert_eq!(player.edges[0].label, "owns");
        assert_eq!(player.edges[0].target_type, "item");
        assert_eq!(player.edges[0].target_id, "sword");
    }

    #[test]
    fn test_parse_inline_edge() {
        let input = "@player:andrew --[owns]--> @item:sword";
        let graph = parse(input);
        
        let player = graph.get("player", "andrew");
        assert!(player.is_some());
        
        let node = player.unwrap();
        assert_eq!(node.edges.len(), 1);
        assert_eq!(node.edges[0].label, "owns");
        assert_eq!(node.edges[0].target_type, "item");
        assert_eq!(node.edges[0].target_id, "sword");
    }

    #[test]
    fn test_round_trip() {
        let input = "
    @player:andrew
    courage: 14
    class: \"Compensated Anarchist\"
    --[owns]--> @item:sword

    @item:sword
    damage: 8
    ";
        let graph = parse(input);
        let serialized = serialize(&graph);
        let reparsed = parse(&serialized);

        // both graphs should have the same nodes
        assert_eq!(graph.nodes.len(), reparsed.nodes.len());
        
        // player should survive the round trip intact
        let original = graph.get("player", "andrew").unwrap();
        let restored = reparsed.get("player", "andrew").unwrap();
        assert_eq!(original.props.len(), restored.props.len());
        assert_eq!(original.edges.len(), restored.edges.len());
    }
}