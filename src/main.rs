mod graph;
mod query;
mod parser;
mod serializer;
mod signal;
mod server;
mod state;

use parser::parse;
use state::AppState;

#[tokio::main]
async fn main() {
    let world = parse("
@player:andrew
  threshold: 0.3
  activation: 0.0
  --[near]--> @npc:guard

@npc:guard
  threshold: 0.4
  activation: 0.0
  --[reports_to]--> @npc:commander

@npc:commander
  threshold: 0.3
  activation: 0.0
  ");

    let state = AppState::new(world);
    server::start(state).await;
}