use crate::graph::{ESGraph, ESNode, ESValue};
use crate::state::AppState;
use crate::{load_world_dir, repair_world};

/// The seed world with stat blocks derived the way main() does it.
fn seeded_world() -> ESGraph {
    let mut world = load_world_dir("data/world");
    let npc_keys: Vec<String> = world
        .nodes
        .keys()
        .filter(|k| k.starts_with("npc:"))
        .cloned()
        .collect();
    for key in npc_keys {
        let complaints = crate::stats::refresh_stat_block(&mut world, &key);
        assert!(
            complaints.is_empty(),
            "seed world NPC {} has incomplete grades: {:?}",
            key,
            complaints
        );
    }
    world
}

// ── Seed world integrity ──────────────────────────────────────

/// Guards the seed world against the failure modes that are silent at
/// runtime: a mistyped prop name, or an edge pointing at nothing.
#[test]
fn test_seed_world_is_well_formed() {
    let graph = load_world_dir("data/world");
    assert!(!graph.nodes.is_empty(), "data/world produced an empty graph");

    let player = graph
        .get("world", "player", "andrew")
        .expect("player:andrew missing from seed world");
    assert!(
        matches!(player.props.get("location"), Some(ESValue::Text(_))),
        "player needs a `location` prop — the engine reads the prop, not the located_in edge"
    );

    // Any NPC without a location prop is invisible to NEARBY and to ambient
    // propagation, and nothing reports it.
    for (key, node) in &graph.nodes {
        if node.node_type != "npc" {
            continue;
        }
        assert!(
            matches!(node.props.get("location"), Some(ESValue::Text(_))),
            "{} has no `location` prop — it will never appear in NEARBY",
            key
        );
    }

    // Every world edge must land on a node that exists.
    for (key, node) in &graph.nodes {
        if !ESGraph::is_world_key(key) {
            continue;
        }
        for edge in &node.edges {
            let target = format!("{}:{}", edge.target_type, edge.target_id);
            assert!(
                graph.nodes.contains_key(&target),
                "{} has --[{}]--> {} pointing at a node that does not exist",
                key,
                edge.label,
                target
            );
        }
    }
}

// ── Repair pass ───────────────────────────────────────────────

#[test]
fn test_repair_drops_impossible_characters() {
    let mut world = ESGraph::new();
    world.insert(ESNode::new("world", "player", "andrew"));
    world.insert(ESNode::new("world", "npc", "andrew")); // duplicate identity
    world.insert(ESNode::new("world", "npc", "player")); // type word as a name
    world.insert(
        ESNode::new("world", "npc", "john_smith")
            .with_prop("occupation", ESValue::Text("guard".to_string())),
    );

    repair_world(&mut world);

    assert!(!world.nodes.contains_key("npc:andrew"), "npc:andrew duplicates player:andrew");
    assert!(!world.nodes.contains_key("npc:player"), "`player` is not a name");
    assert!(world.nodes.contains_key("npc:john_smith"), "a real NPC must survive");
    assert!(world.nodes.contains_key("player:andrew"), "the player must survive");
}

#[test]
fn test_repair_prunes_dangling_edges() {
    let mut world = ESGraph::new();
    world.insert(ESNode::new("world", "location", "guard_headquarters"));
    world.insert(
        ESNode::new("world", "npc", "elias_roth")
            .with_edge("located_in", "location", "guard_headquarters") // real
            .with_edge("occupied_by", "npc", "andrew")                 // never existed
            .with_edge("watches", "location", "market"),               // never existed
    );

    repair_world(&mut world);

    let elias = world.get("world", "npc", "elias_roth").unwrap();
    assert_eq!(elias.edges.len(), 1, "only the edge with a real target survives");
    assert_eq!(elias.edges[0].target_id, "guard_headquarters");
}

// ── End-to-end propagation over the real seed world ───────────

/// Every node may be signalled at most once per propagation.
///
/// The visited set used to be cloned per branch, so a node was reachable again
/// down every other path — and since absorbing raises awareness and lowers the
/// threshold, NPCs re-absorbed their own echoes and queued more continuations.
/// The frontier grew instead of shrinking, and with a sleep per dequeue one
/// action took minutes.
#[tokio::test]
async fn test_each_node_is_signalled_once() {
    let state = AppState::new_without_db(seeded_world());
    let mut rx = state.tx.subscribe();

    let signal = crate::signal::EventSignal::new(
        "player:andrew",
        0.8,
        "draws a blade in the middle of the market",
    );
    let _ = crate::signal::propagate(state.clone(), signal).await;

    let mut targets: Vec<String> = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let crate::server::ServerMessage::SignalHop { to, .. } = msg {
            targets.push(to);
        }
    }

    let unique: std::collections::HashSet<&String> = targets.iter().collect();
    assert_eq!(
        targets.len(),
        unique.len(),
        "a node was signalled more than once: {:?}",
        targets
    );
    assert!(!targets.is_empty(), "propagation emitted no hops at all");
}

/// A shout from an NPC has to reach the other people standing there.
///
/// Cascades used to inherit the originating signal's visited set, which
/// excluded everyone who had already heard the first event — so a guard
/// yelling "Stop, thief!" reached precisely nobody, every single time.
#[tokio::test]
async fn test_an_npc_shout_reaches_the_others() {
    let state = AppState::new_without_db(seeded_world());

    let shout = crate::signal::EventSignal::new("npc:john_smith", 0.8, "shouts: Stop, thief!");
    let (heard, _) = crate::signal::propagate(state, shout).await;

    let names: Vec<&str> = heard.iter().map(|a| a.npc_id.as_str()).collect();
    println!("shout heard by: {:?}", names);

    assert!(
        names.contains(&"npc:jin_lyons"),
        "the pickpocket standing right there should hear a shout, got {:?}",
        names
    );
    assert!(
        names.contains(&"npc:thomas_pellar"),
        "the merchant standing right there should hear a shout, got {:?}",
        names
    );
    assert!(
        !names.contains(&"npc:john_smith"),
        "an NPC must not hear its own shout, got {:?}",
        names
    );
}

/// End-to-end: a loud event in the market must actually be heard by the NPCs
/// standing in the market. This is the behaviour the whole signal system exists
/// for, and it was silently broken.
#[tokio::test]
async fn test_market_event_is_heard() {
    let state = AppState::new_without_db(seeded_world());
    let signal = crate::signal::EventSignal::new(
        "player:andrew",
        0.8,
        "draws a blade in the middle of the market",
    );
    let (absorbed, _) = crate::signal::propagate(state.clone(), signal).await;

    // The visualizer renders awareness from this prop. Stat blocks live in a
    // private namespace and never reach the browser, so if this stops being
    // stamped the glow silently falls back to a guessed baseline.
    {
        let graph = state.graph.read().await;
        for (key, node) in &graph.nodes {
            if node.node_type != "npc" {
                continue;
            }
            assert!(
                node.get_number("awareness_baseline").is_some(),
                "{} has no awareness_baseline — the visualizer cannot render it",
                key
            );
        }
    }

    let heard: Vec<&str> = absorbed.iter().map(|a| a.npc_id.as_str()).collect();
    println!("heard by: {:?}", heard);

    assert!(
        heard.contains(&"npc:john_smith"),
        "the guard standing in the market should notice a drawn blade, got {:?}",
        heard
    );
    assert!(
        heard.contains(&"npc:jin_lyons"),
        "the pickpocket standing in the market should notice, got {:?}",
        heard
    );
    assert!(
        heard.contains(&"npc:thomas_pellar"),
        "the merchant standing in the market should notice, got {:?}",
        heard
    );
}
