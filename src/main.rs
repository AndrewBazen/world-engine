mod graph;
mod memory;
mod signal;
mod server;
mod state;
mod stats;
mod agent;
mod llm;
mod db;

use crate::graph::{parse, ESGraph, ESNode};
use state::AppState;

fn load_world_dir(path: &str) -> ESGraph {
    let mut combined = ESGraph::new();
    
    load_es_files_recursive(path, &mut combined);
    
    combined
}

fn load_es_files_recursive(dir: &str, graph: &mut ESGraph) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_es_files_recursive(path.to_str().unwrap_or(""), graph);
        } else if path.extension().and_then(|e| e.to_str()) == Some("es") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| {
                    eprintln!("failed to read {:?}: {}", path, e);
                    String::new()
                });
            let patch = parse(&content);
            // merge into combined graph
            for (key, node) in patch.nodes {
                graph.nodes.insert(key, node);
            }
            println!("loaded {:?}", path);
        }
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::graph::ESValue;

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

        // Any NPC without a location prop is invisible to NEARBY and to
        // ambient propagation, and nothing reports it.
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

    /// The seed world with stat blocks generated the way main() does it.
    fn seeded_world() -> ESGraph {
        let mut world = load_world_dir("data/world");
        let npcs: Vec<(String, ESNode)> = world
            .nodes
            .iter()
            .filter(|(k, _)| k.starts_with("npc:"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, node) in npcs {
            let npc_id = key.split(':').nth(1).unwrap_or("").to_string();
            let stats = crate::stats::generate_stats(&node);
            crate::stats::write_stat_block(&mut world, &npc_id, &stats);
        }
        world
    }

    #[test]
    fn test_repair_drops_impossible_characters() {
        use crate::graph::ESValue;

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

    /// Every node may be signalled at most once per propagation.
    ///
    /// The visited set used to be cloned per branch, so a node was reachable
    /// again down every other path — and since absorbing raises awareness and
    /// lowers the threshold, NPCs re-absorbed their own echoes and queued more
    /// continuations. The frontier grew instead of shrinking, and with a sleep
    /// per dequeue one action took minutes.
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

        let shout = crate::signal::EventSignal::new(
            "npc:john_smith",
            0.8,
            "shouts: Stop, thief!",
        );
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

    /// End-to-end: a loud event in the market must actually be heard by the
    /// NPCs standing in the market. This is the behaviour the whole signal
    /// system exists for, and it was silently broken.
    #[tokio::test]
    async fn test_market_event_is_heard() {
        let state = AppState::new_without_db(seeded_world());
        let signal = crate::signal::EventSignal::new(
            "player:andrew",
            0.8,
            "draws a blade in the middle of the market",
        );
        let (absorbed, _) = crate::signal::propagate(state.clone(), signal).await;

        // The visualizer renders awareness from this prop. Stat blocks live in
        // a private namespace and never reach the browser, so if this stops
        // being stamped the glow silently falls back to a guessed baseline.
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
}

/// Drop nodes that cannot legitimately exist, and report the ones that merely
/// look wrong. Worlds persisted before the write guards existed accumulated
/// characters and places that no author ever wrote — an `@npc:andrew` beside
/// `@player:andrew`, a `@location:market` beside `@location:market_district` —
/// and every one of them keeps absorbing signals forever.
fn repair_world(world: &mut ESGraph) {
    let character_keys: Vec<String> = world
        .nodes
        .keys()
        .filter(|k| k.starts_with("npc:"))
        .cloned()
        .collect();

    let mut dropped = 0;
    for key in character_keys {
        let name = match key.split(':').nth(1) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // the same person cannot be both a player and an NPC
        if world.nodes.contains_key(&format!("player:{}", name)) {
            println!("repair: dropping {} (duplicates player:{})", key, name);
            world.nodes.remove(&key);
            world.nodes.remove(&format!("stats/{}/stats:block", name));
            dropped += 1;
            continue;
        }

        // a type word used as a name
        if matches!(name.as_str(), "player" | "npc" | "unknown" | "someone") {
            println!("repair: dropping {} (type word, not a name)", key);
            world.nodes.remove(&key);
            world.nodes.remove(&format!("stats/{}/stats:block", name));
            dropped += 1;
        }
    }

    // Edges pointing at nodes that do not exist. These are invisible in the
    // graph view but very loud in the signal log — propagation used to emit a
    // hop toward each one on every pass.
    let world_keys: Vec<String> = world.nodes.keys().cloned().collect();
    let mut pruned_edges = 0;
    for key in &world_keys {
        if !ESGraph::is_world_key(key) {
            continue;
        }
        let dangling: Vec<(String, String, String)> = match world.nodes.get(key) {
            Some(node) => node
                .edges
                .iter()
                .filter(|e| {
                    !world
                        .nodes
                        .contains_key(&format!("{}:{}", e.target_type, e.target_id))
                })
                .map(|e| (e.label.clone(), e.target_type.clone(), e.target_id.clone()))
                .collect(),
            None => continue,
        };

        if dangling.is_empty() {
            continue;
        }
        if let Some(node) = world.nodes.get_mut(key) {
            for (label, ty, id) in &dangling {
                println!("repair: dropping dangling edge {} --[{}]--> {}:{}", key, label, ty, id);
                pruned_edges += 1;
            }
            node.edges.retain(|e| {
                !dangling
                    .iter()
                    .any(|(l, t, i)| &e.label == l && &e.target_type == t && &e.target_id == i)
            });
        }
    }

    // Anything else that looks like debris gets reported rather than deleted —
    // an empty location may simply be one you have not written yet.
    for (key, node) in &world.nodes {
        if !ESGraph::is_world_key(key) {
            continue;
        }
        if node.props.is_empty() && node.edges.is_empty() {
            println!("repair: note — {} has no properties and no edges", key);
        }
    }

    if dropped > 0 || pruned_edges > 0 {
        println!(
            "repair: removed {} invalid node(s) and {} dangling edge(s)",
            dropped, pruned_edges
        );
    }
}

fn main_setup() -> (ESGraph, redb::Database) {
    let db = db::connect().expect("failed to connect to db");
    
    let graph = match db::load_graph(&db) {
        Ok(g) if !g.nodes.is_empty() => {
            println!("loaded world from db ({} nodes)", g.nodes.len());
            g
        }
        _ => {
            println!("loading fresh world from data/world/");
            let fresh = load_world_dir("data/world");
            db::save_graph(&db, &fresh).expect("failed to save initial world");
            fresh
        }
    };

    (graph, db)
}

#[tokio::main]
async fn main() {
    let (mut world, db) = main_setup();

    repair_world(&mut world);

    // generate missing stat blocks before creating state
    let new_npcs: Vec<(String, ESNode)> = world.nodes.iter()
        .filter(|(k, _)| k.starts_with("npc:"))
        .filter(|(k, _)| {
            let npc_id = k.split(':').nth(1).unwrap_or("");
            !crate::stats::has_stat_block(&world, npc_id)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // in main, before generating stat blocks
    println!("checking for existing stat blocks...");
    let existing_stats = world.nodes.keys()
        .filter(|k| k.starts_with("stats/"))
        .count();
    println!("found {} existing stat nodes", existing_stats);

    for (key, node) in new_npcs {
        let npc_id = key.split(':').nth(1).unwrap_or("").to_string();
        let stats = crate::stats::generate_stats(&node);
        crate::stats::write_stat_block(&mut world, &npc_id, &stats);
        println!("generated stat block for {}", npc_id);
    }

    // now world moves into state — all stat blocks already written
    let state = AppState::new(world, db);
    server::start(state).await;
}