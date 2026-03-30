use crate::graph::{ESGraph, ESNode, ESValue, ESEdge};

enum LineType {
    Comment,
    InlineEdge,
    NodeDecl,
    Edge,
    Property,
    Empty
}

fn classify(line: &str) -> LineType {
    if line.starts_with('#') { LineType::Comment }
    else if line.starts_with('@') && line.contains("--[") { LineType::InlineEdge }
    else if line.starts_with('@') { LineType::NodeDecl }
    else if line.starts_with("--[") { LineType::Edge }
    else if line.contains(": ") { LineType::Property }
    else { LineType::Empty }
}

pub fn parse(input: &str) -> ESGraph {
    let mut graph = ESGraph::new();
    let mut current: Option<ESNode> = None;
    
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        match classify(line) {
            LineType::Empty      => { continue; }
            LineType::Comment    => { continue; }
            LineType::NodeDecl => {
                if let Some(node) = current.take() {
                    graph.insert(node);
                }
            
                let line = line.trim_start_matches('@');
                
                // check for namespace prefix
                let (namespace, type_and_id) = if line.contains('/') {
                    // find the last '/' — everything before is namespace, after is type:id
                    let last_slash = line.rfind('/').unwrap();
                    (&line[..last_slash], &line[last_slash + 1..])
                } else {
                    ("world", line)
                };
            
                let parts: Vec<&str> = type_and_id.splitn(2, ':').collect();
                if parts.len() < 2 {
                    eprintln!("skipping malformed node declaration: {}", line);
                    continue;
                }
                let n_type = parts[0];
                let n_id = parts[1];
            
                current = Some(ESNode::new(namespace, n_type, n_id));
            }
            LineType::Property   => {
                if let Some(node) = current.as_mut() {
                    
                    let parts: Vec<&str> = line.splitn(2, ": ").collect();
                    if parts.len() < 2 {
                        eprintln!("skipping malformed property declaration: {}", line);
                        continue;
                    }
                    let key = parts[0].trim();
                    let raw = parts[1].trim();

                    let value = if raw == "true" {
                        ESValue::Bool(true)
                    } else if raw == "false" {
                        ESValue::Bool(false)
                    } else if let Ok(n) = raw.parse::<f64>() {
                        ESValue::Number(n)
                    } else {
                        ESValue::Text(raw.trim_matches('"').to_string())
                    };

                    node.props.insert(key.to_string(), value);
                }
            }
            LineType::Edge       => {
                if let Some(node) = current.as_mut() {
                    let line = line.strip_prefix("--[").unwrap_or(line);

                    let parts: Vec<&str> = line.splitn(2, "]-->").collect();
                    let label = parts[0];

                    let mut target = parts[1].trim();
                    target = target.trim_start_matches("@");
                    let target_parts: Vec<&str> = target.splitn(2, ":").collect();
                    if target_parts.len() < 2 {
                        eprintln!("skipping malformed edge target: {}", target);
                        continue;
                    }
                    let target_type = target_parts[0];
                    let target_id = target_parts[1];

                    node.edges.push(ESEdge::new(label, target_type, target_id));
                }
            }
            LineType::InlineEdge => {
                // split the inline edge
                let inline_parts: Vec<&str> = line.splitn(2, " --[").collect();

                // parse the node declaration
                if let Some(node) = current.take() {
                    graph.insert(node);
                }
                let line = inline_parts[0].trim_start_matches("@");

                // check for namespace prefix
                let (namespace, type_and_id) = if line.contains('/') {
                    // find the last '/' — everything before is namespace, after is type:id
                    let last_slash = line.rfind('/').unwrap();
                    (&line[..last_slash], &line[last_slash + 1..])
                } else {
                    ("world", line)
                };
                let n_parts: Vec<&str> = type_and_id.splitn(2, ":").collect();
                if n_parts.len() < 2 {
                    eprintln!("skipping malformed inline node declaration: {}", line);
                    continue;
                }
                let n_type = n_parts[0];
                let n_id = n_parts[1];

                current = Some(ESNode::new(namespace, n_type, n_id));

                // parse the edge
                if let Some(node) = current.as_mut() {
                    let parts: Vec<&str> = inline_parts[1].splitn(2, "]-->").collect();
                    let label = parts[0];

                    let mut target = parts[1].trim();
                    target = target.trim_start_matches("@");
                    let target_parts: Vec<&str> = target.splitn(2, ":").collect();
                    if target_parts.len() < 2 {
                        eprintln!("skipping malformed edge target: {}", target);
                        continue;
                    }
                    let target_type = target_parts[0];
                    let target_id = target_parts[1];

                    node.edges.push(ESEdge::new(label, target_type, target_id));
                }

                // flush current
                if let Some(node) = current.take() {
                    graph.insert(node);
                }
            }
        }
    }

    if let Some(node) = current.take() {
        graph.insert(node);
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::serialize;

    #[test]
    fn test_parse_node_declaration() {
        let input = "@player:andrew";

        let graph = parse(input);
        
        assert!(graph.nodes.contains_key("player:andrew"));
        let retrieved = graph.get("world", "player", "andrew");
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

        let retrieved = graph.get("world", "player", "andrew");
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
        let player = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(player.edges.len(), 1);
        assert_eq!(player.edges[0].label, "owns");
        assert_eq!(player.edges[0].target_type, "item");
        assert_eq!(player.edges[0].target_id, "sword");
    }

    #[test]
    fn test_parse_inline_edge() {
        let input = "@player:andrew --[owns]--> @item:sword";
        let graph = parse(input);
        
        let player = graph.get("world", "player", "andrew");
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
        let original = graph.get("world", "player", "andrew").unwrap();
        let restored = reparsed.get("world", "player", "andrew").unwrap();
        assert_eq!(original.props.len(), restored.props.len());
        assert_eq!(original.edges.len(), restored.edges.len());
    }

    #[test]
    fn test_parse_namespaced_node() {
        let input = "
    @inventory/andrew/item:sword
    name: \"Ancient Sword\"
    damage: 15
    ";
        let graph = parse(input);
        
        let key = "inventory/andrew/item:sword";
        let node = graph.nodes.get(key);
        assert!(node.is_some());
        
        let node = node.unwrap();
        assert_eq!(node.namespace, "inventory/andrew");
        assert_eq!(node.node_type, "item");
        assert_eq!(node.id, "sword");
        assert!(matches!(
            node.props.get("damage"),
            Some(ESValue::Number(v)) if *v == 15.0
        ));
    }
}