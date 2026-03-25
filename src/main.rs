mod graph;
mod query;
mod parser;
mod serializer;
mod signal;
mod server;
mod state;
mod agent;

use parser::parse;
use state::AppState;

#[tokio::main]
async fn main() {
  let world = parse("
@player:andrew
  threshold: 0.2
  activation: 0.0
  courage: 14
  class: \"Compensated Anarchist\"
  race: \"Primal\"
  narrative: \"A newcomer. No history yet.\"
  dominant_trait: \"unknown\"
  notable_actions: \"none\"
  location: \"market_district\"
  
  @npc:guard
    location: \"market_district\"
    threshold: 0.4
    activation: 0.0
    --[reports_to]--> @npc:commander
    --[member_of]--> @faction:garrison

  
  @npc:commander
    location: \"garrison_corridor\"
    threshold: 0.3
    activation: 0.0
    --[commands]--> @faction:garrison
  
  @npc:merchant
    location: \"market_district\"
    threshold: 0.6
    activation: 0.0
    --[member_of]--> @faction:trade_guild
  
  @faction:garrison
    threshold: 0.3
    activation: 0.0
  
  @faction:trade_guild
    threshold: 0.7
    activation: 0.0
  ");
  let state = AppState::new(world);
  server::start(state).await;
}