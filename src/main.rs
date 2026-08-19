mod graph;
mod memory;
mod signal;
mod server;
mod state;
mod stats;
mod agent;
mod llm;
mod db;
#[cfg(test)] mod tests;


use crate::{graph::{ESGraph, ESNode, parse}, stats::{read_grades, refresh_stat_block}};
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

fn main_setup() -> (ESGraph, redb::Database, u64) {
    let db = db::connect().expect("failed to connect to db");
    
    let (graph, turn) = match db::load_graph(&db) {
        Ok(g) if !g.nodes.is_empty() => {
            let turn = db::load_turn(&db).unwrap_or(0);
            println!("loaded world from db ({} nodes, turn {})", g.nodes.len(), turn);
            (g, turn)
        }
        _ => {
            println!("loading fresh world from data/world/");
            let fresh = load_world_dir("data/world");
            db::save_graph(&db, &fresh).expect("failed to save initial world");
            db::save_turn(&db, 0).expect("failed to reset turn");
            (fresh, 0)
        }
    };

    (graph, db, turn)
}

#[tokio::main]
async fn main() {
    let (mut world, db, turn) = main_setup();

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

    for key in new_npcs {
        let complaints = refresh_stat_block(&mut world, &key.0);
        if complaints.is_empty() {
            println!("stats for {}", key.0);
        } else {
            println!("stats for {} - {} problems(s):", key.0, complaints.len());
            for c in &complaints { println!("    {}", c); }
        }
    }

    // now world moves into state — all stat blocks already written
    let state = AppState::new(world, db, turn);
    server::start(state).await;
}
