mod graph;
mod query;
mod parser;
mod serializer;
mod signal;
mod server;
mod state;
mod agent;
mod db;

use parser::parse;
use state::AppState;

#[tokio::main]
async fn main() {
    // connect to database
    let database = db::connect().await.expect("failed to connect to db");

    // try loading world from db, fall back to fresh world
    let world = match db::load_graph(&database).await {
        Ok(g) if !g.nodes.is_empty() => {
            println!("loaded world from db ({} nodes)", g.nodes.len());
            g
        }
        _ => {
            println!("creating fresh world");
            let fresh = parse("
@player:andrew
  threshold: 0.2
  activation: 0.0
  courage: 14
  class: \"Compensated Anarchist\"
  narrative: \"A newcomer. No history yet.\"
  dominant_trait: \"unknown\"
  notable_actions: \"none\"
  location: \"market_district\"
  region: \"mirefall_city\"

@npc:guard
  threshold: 0.4
  activation: 0.0
  disposition: neutral
  awareness: 0.7
  personality: vigilant
  location: \"market_district\"
  region: \"mirefall_city\"
  reaction_threshold: 0.4
  --[reports_to]--> @npc:commander
  --[knows]--> @npc:merchant
  --[member_of]--> @faction:garrison

@npc:merchant
  threshold: 0.6
  activation: 0.0
  disposition: neutral
  awareness: 0.3
  inventory_size: large
  personality: cautious
  location: \"market_district\"
  region: \"mirefall_city\"
  reaction_threshold: 0.65
  --[member_of]--> @faction:trade_guild
  --[knows]--> @npc:guard

@npc:commander
  threshold: 0.3
  activation: 0.0
  disposition: neutral
  awareness: 0.8
  personality: authoritative
  location: \"garrison_corridor\"
  region: \"mirefall_city\"
  reaction_threshold: 0.35
  --[commands]--> @faction:garrison
  --[member_of]--> @faction:garrison

@npc:farmer
  threshold: 0.5
  activation: 0.0
  disposition: neutral
  awareness: 0.2
  personality: simple
  location: \"southern_fields\"
  region: \"mirefall_city\"
  reaction_threshold: 0.8

@faction:garrison
  threshold: 0.3
  activation: 0.0
  region: \"mirefall_city\"

@faction:trade_guild
  threshold: 0.7
  activation: 0.0
  region: \"mirefall_city\"

@inventory/andrew/item:worn_dagger
  name: \"Worn Dagger\"
  damage: 3
  rarity: common
  --[owned_by]--> @player:andrew

@abilities/andrew/ability:stealth
  name: \"Stealth\"
  level: 1
  description: \"Move unseen through shadows\"
");
            // save fresh world to db
            db::save_graph(&database, &fresh).await
                .expect("failed to save initial world");
            fresh
        }
    };

    let state = AppState::new(world, database);

    server::start(state).await;
}