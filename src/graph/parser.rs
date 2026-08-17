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

/// Split a node declaration into (namespace, "type:id").
/// `inventory/andrew/item:sword` → ("inventory/andrew", "item:sword")
/// `player:andrew`               → ("world", "player:andrew")
fn split_namespace(decl: &str) -> (&str, &str) {
    match decl.rfind('/') {
        Some(i) => (&decl[..i], &decl[i + 1..]),
        None => ("world", decl),
    }
}

/// Split an edge line into (label, raw target). Tolerates the arrow being
/// written with one or two dashes, which models get wrong regularly.
/// Returns None rather than panicking on anything else.
fn split_edge(line: &str) -> Option<(&str, &str)> {
    let body = line.trim_start().strip_prefix("--[")?;
    for arrow in ["]-->", "]->", "]-"] {
        if let Some(pair) = body.split_once(arrow) {
            return Some(pair);
        }
    }
    None
}

/// Parse an edge target into (type, id), discarding any namespace prefix so
/// `@inventory/andrew/item:sword` yields ("item", "sword") rather than a
/// target_type containing the whole path.
fn parse_edge_target(raw: &str) -> Option<(&str, &str)> {
    let t = raw.trim().trim_start_matches('@');
    let t = match t.rfind('/') {
        Some(i) => &t[i + 1..],
        None => t,
    };
    let (ty, id) = t.split_once(':')?;
    let (ty, id) = (ty.trim(), id.trim());
    if ty.is_empty() || id.is_empty() {
        return None;
    }
    Some((ty, id))
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
                    graph.merge_node(node);
                }
            
                let line = line.trim_start_matches('@');

                let (namespace, type_and_id) = split_namespace(line);

                let Some((n_type, n_id)) = type_and_id.split_once(':') else {
                    eprintln!("skipping malformed node declaration: {}", line);
                    continue;
                };

                current = Some(ESNode::new(namespace, n_type.trim(), n_id.trim()));
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
                let Some(node) = current.as_mut() else {
                    eprintln!("skipping edge with no node declared above it: {}", line);
                    continue;
                };

                let Some((label, target_raw)) = split_edge(line) else {
                    eprintln!("skipping malformed edge: {}", line);
                    continue;
                };

                match parse_edge_target(target_raw) {
                    Some((target_type, target_id)) => {
                        node.edges.push(ESEdge::new(label.trim(), target_type, target_id));
                    }
                    None => {
                        eprintln!("skipping malformed edge target: {}", target_raw.trim());
                    }
                }
            }
            LineType::InlineEdge => {
                // "@node:decl --[label]--> @target:id" — split at the arrow.
                // Note: no leading space required, models often omit it.
                let Some(arrow_at) = line.find("--[") else {
                    eprintln!("skipping malformed inline edge: {}", line);
                    continue;
                };
                let (decl_part, edge_part) = line.split_at(arrow_at);

                // flush whatever node we were building
                if let Some(node) = current.take() {
                    graph.merge_node(node);
                }

                let decl = decl_part.trim().trim_start_matches('@');
                let (namespace, type_and_id) = split_namespace(decl);

                let Some((n_type, n_id)) = type_and_id.split_once(':') else {
                    eprintln!("skipping malformed inline node declaration: {}", decl);
                    continue;
                };

                let mut node = ESNode::new(namespace, n_type.trim(), n_id.trim());

                match split_edge(edge_part).and_then(|(label, target_raw)| {
                    parse_edge_target(target_raw).map(|(ty, id)| (label, ty, id))
                }) {
                    Some((label, target_type, target_id)) => {
                        node.edges.push(ESEdge::new(label.trim(), target_type, target_id));
                    }
                    None => {
                        eprintln!("skipping malformed inline edge target: {}", edge_part.trim());
                    }
                }

                graph.merge_node(node);
            }
        }
    }

    if let Some(node) = current.take() {
        graph.merge_node(node);
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::serialize;

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
    fn test_redeclared_node_merges_instead_of_replacing() {
        // Models declare the same node several times in one patch constantly.
        // The earlier block used to be discarded outright.
        let input = "
@quests/andrew/quest:steal_sword
description: \"Steal the legendary sword\"
status: active

@quests/andrew/quest:steal_sword
narrative: \"The legend speaks of a sword.\"
";
        let graph = parse(input);
        assert_eq!(graph.nodes.len(), 1);

        let quest = graph.nodes.get("quests/andrew/quest:steal_sword").unwrap();
        assert!(quest.props.contains_key("description"), "first block must survive");
        assert!(quest.props.contains_key("status"), "first block must survive");
        assert!(quest.props.contains_key("narrative"), "second block must apply");
    }

    #[test]
    fn test_redeclared_node_merges_edges_without_duplicating() {
        let input = "
@player:andrew
--[owns]--> @item:sword

@player:andrew
--[owns]--> @item:sword
--[owns]--> @item:shield
";
        let graph = parse(input);
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges.len(), 2, "duplicate edge must not be added twice");
    }

    // ── Malformed input must never panic ─────────────────────

    #[test]
    fn test_edge_with_no_arrow_is_skipped() {
        let graph = parse("@player:andrew\n--[owns] @item:sword\n");
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges.len(), 0);
    }

    #[test]
    fn test_single_dash_arrow_is_tolerated() {
        let graph = parse("@player:andrew\n--[owns]-> @item:sword\n");
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges.len(), 1);
    }

    #[test]
    fn test_edge_before_any_node_is_skipped() {
        let graph = parse("--[owns]--> @item:sword\n@player:andrew\n");
        assert!(graph.get("world", "player", "andrew").is_some());
    }

    #[test]
    fn test_inline_edge_without_leading_space() {
        let graph = parse("@player:andrew--[owns]--> @item:sword");
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges.len(), 1);
        assert_eq!(p.edges[0].target_type, "item");
        assert_eq!(p.edges[0].target_id, "sword");
    }

    #[test]
    fn test_garbage_edge_target_is_skipped() {
        let graph = parse("@player:andrew\n--[owns]--> @inventory end\n");
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges.len(), 0);
    }

    // ── Edge targets resolve to a clean type/id ───────────────

    #[test]
    fn test_location_edge_target_parses() {
        let graph = parse("@player:andrew\n--[located_in]--> @location:market_district\n");
        let p = graph.get("world", "player", "andrew").unwrap();
        assert_eq!(p.edges[0].target_type, "location");
        assert_eq!(p.edges[0].target_id, "market_district");
    }

    #[test]
    fn test_namespaced_edge_target_drops_the_namespace() {
        let graph = parse(
            "@equipped/andrew/equipped:slots\n--[main_hand]--> @inventory/andrew/item:sword\n",
        );
        let node = graph.nodes.get("equipped/andrew/equipped:slots").unwrap();
        assert_eq!(node.edges[0].target_type, "item");
        assert_eq!(node.edges[0].target_id, "sword");
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