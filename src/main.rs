mod graph;
mod query;
mod parser;
mod serializer;
mod signal;
mod server;
mod state;
mod stats;
mod agent;
mod db;

use graph::{ESGraph, ESNode};
use parser::parse;
use state::AppState;

const WORLD_FILE: &str = "data/world.es";

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