use super::*;

// ── ESGraph / ESNode ──────────────────────────────────────────

#[test]
fn test_basic_node_creation() {
    let mut graph = ESGraph::new();

    let player = ESNode::new("world", "player", "andrew")
        .with_prop("courage", ESValue::Number(14.0))
        .with_prop("name", ESValue::Text("Andrew".to_string()))
        .with_edge("playing", "session", "s1");

    graph.insert(player);

    let node = graph
        .get("world", "player", "andrew")
        .expect("player should be retrievable");

    assert_eq!(node.id, "andrew");
    assert_eq!(node.node_type, "player");
    assert_eq!(node.namespace, "world");
    assert!(matches!(node.props.get("courage"), Some(ESValue::Number(v)) if *v == 14.0));
    assert_eq!(node.edges.len(), 1);
    assert_eq!(node.edges[0].label, "playing");
    assert_eq!(node.edges[0].target_id, "s1");
}

#[test]
fn test_missing_node_returns_none() {
    let graph = ESGraph::new();
    assert!(graph.get("world", "player", "nobody").is_none());
}

#[test]
fn test_namespace_key_format() {
    assert_eq!(ESGraph::make_key("world", "player", "andrew"), "player:andrew");
    assert_eq!(
        ESGraph::make_key("inventory/andrew", "item", "sword"),
        "inventory/andrew/item:sword"
    );
}

#[test]
fn test_is_world_key() {
    assert!(ESGraph::is_world_key("player:andrew"));
    assert!(ESGraph::is_world_key("npc:guard"));
    assert!(!ESGraph::is_world_key("inventory/andrew/item:sword"));
    assert!(!ESGraph::is_world_key("memory/guard/event:theft"));
}

// ── Parser: the happy path ────────────────────────────────────

#[test]
fn test_parse_node_declaration() {
    let graph = parse("@player:andrew");

    assert!(graph.nodes.contains_key("player:andrew"));
    let node = graph.get("world", "player", "andrew").unwrap();
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
    let node = graph.get("world", "player", "andrew").unwrap();

    assert_eq!(node.props.len(), 3);
    assert!(matches!(node.props.get("class"), Some(ESValue::Text(s)) if s == "Compensated Anarchist"));
    assert!(matches!(node.props.get("strength"), Some(ESValue::Number(v)) if *v == 12.0));
    assert!(matches!(node.props.get("alive"), Some(ESValue::Bool(b)) if *b));
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
    let graph = parse("@player:andrew --[owns]--> @item:sword");
    let node = graph.get("world", "player", "andrew").unwrap();

    assert_eq!(node.edges.len(), 1);
    assert_eq!(node.edges[0].label, "owns");
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
    let node = graph.nodes.get("inventory/andrew/item:sword").unwrap();

    assert_eq!(node.namespace, "inventory/andrew");
    assert_eq!(node.node_type, "item");
    assert_eq!(node.id, "sword");
    assert!(matches!(node.props.get("damage"), Some(ESValue::Number(v)) if *v == 15.0));
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
    let reparsed = parse(&serialize(&graph));

    assert_eq!(graph.nodes.len(), reparsed.nodes.len());

    let original = graph.get("world", "player", "andrew").unwrap();
    let restored = reparsed.get("world", "player", "andrew").unwrap();
    assert_eq!(original.props.len(), restored.props.len());
    assert_eq!(original.edges.len(), restored.edges.len());
}

// ── Parser: redeclaration merges rather than replaces ─────────

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

// ── Parser: malformed input must never panic ──────────────────

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

// ── Parser: edge targets resolve to a clean type/id ───────────

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

// ── Query ─────────────────────────────────────────────────────

#[test]
fn test_follow_outgoing_edges() {
    let mut graph = ESGraph::new();

    let player = ESNode::new("world", "player", "andrew").with_edge("owns", "item", "sword");
    let item = ESNode::new("world", "item", "sword");

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
    graph.insert(ESNode::new("world", "item", "sword").with_edge("owned_by", "player", "andrew"));

    let owned_by = incoming(&graph, "player", "andrew", "owned_by");
    assert_eq!(owned_by.len(), 1);
    assert_eq!(owned_by[0].id, "sword");
}
