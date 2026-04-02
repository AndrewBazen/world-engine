use std::sync::Arc;
use crate::state::AppState;
use crate::graph::ESValue;
use super::PlayerAction;

pub async fn handle_movement(
    state: Arc<AppState>,
    action: PlayerAction,
) -> Result<(), String> {
    // get current location and all available locations
    let (current_location, locations) = {
        let graph = state.graph.read().await;
        let player = graph.nodes.get(&action.player_id)
            .ok_or_else(|| format!("player {} not found", action.player_id))?;

        let current = match player.props.get("location") {
            Some(ESValue::Text(l)) => l.clone(),
            _ => return Err("player has no location".to_string()),
        };

        let locs: Vec<String> = graph.nodes.iter()
            .filter(|(_, n)| n.node_type == "location")
            .map(|(k, _)| k.strip_prefix("location").unwrap_or(k).to_string())
            .collect();

        (current, locs)
    };

    if locations.is_empty() {
        return Err("no locations found in world".to_string());
    }

    // resolve target location from player input
    let target = crate::llm::resolve_location(&action.context, &locations).await?;
    println!("movement resolved: {} -> {}", current_location, target);

    if !locations.contains(&target) {
        return Err(format!("unknown location: {}", target));
    }
    if target == current_location {
        println!("player is already at {}", target);
        return Ok(());
    }

    //signal departure at old location
    let departure = crate::signal::EventSignal::new(
        &action.player_id,
        0.3, 
        &format!("player leaves the {}", current_location),
    ); 
    
    let (_, _)= crate::signal::propagate(state.clone(), departure).await;
    println!("departure signal propagated from {}", current_location);

    //update player location
    {
        let mut graph = state.graph.write().await;
        if let Some(player) = graph.nodes.get_mut(&action.player_id) {
            player.props.insert(
                "location".to_string(),
                ESValue::Text(target.clone())
            );
        }
    }

    // persist
    {
        let graph = state.graph.read().await;
        if let Some(db) = &state.db {
            let db = db.lock().unwrap();
            if let Err(e) = crate::db::save_graph(&*db, &graph) {
                eprintln!("failed to persist after movement: {}", e);
            }
        }
    }

    // signal arrival at new location
    let arrival = crate::signal::EventSignal::new(
        &action.player_id,
        0.5, 
        &format!("a stranger arrives at the {}", target),
    );
    let (_, _) = crate::signal::propagate(state.clone(), arrival).await;
    println!("arrival signal propagated at {}", target);

    // broadcast updated state
    let snapshot = crate::server::build_snapshot(&state).await;
    let _ = state.tx.send(snapshot);

    Ok(())
}