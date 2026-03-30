# world-engine

An AI-driven graph-based RPG backend built in Rust. The world is a living graph of nodes and edges — players, NPCs, factions, items, scenes — connected by relationships and brought to life by a hierarchy of AI agents. Every player action sends an event signal rippling through the graph, and independent NPC agents react based on their own context and personality.

## Core concepts

**Edgescript** is a custom flat graph format designed for LLM-readable world state. Nodes are declared with `@type:id`, properties are indented key-value pairs, and relationships are typed edges. The AI generates world patches in Edgescript and they get merged directly into the live graph.

```
@player:andrew
  courage: 14
  location: "market_district"
  --[located_in]--> @scene:market_district

@inventory/andrew/item:shadow_blade
  name: "Shadow Blade"
  damage: 12
  rarity: rare
  --[owned_by]--> @player:andrew
```

**Event signals** are transient energy that propagate through the graph when something happens. They decay with each hop, travel along edges and location/faction/region proximity, and nodes absorb or ignore them based on their threshold. Nothing is hardcoded — the world reacts based on topology.

**Namespaces** separate world state from private entity state. World nodes (`player:andrew`, `npc:guard`) are visible to all agents and signal propagation. Private namespaces (`inventory/andrew/`, `memory/guard/`, `abilities/andrew/`) are only read by the relevant agent.

**AI agents** operate at different tiers — the player agent handles immediate narrative and consequences, NPC agents wake up when a signal exceeds their reaction threshold and decide independently how to respond, and a world agent handles epoch-scale changes.

## Architecture

```
src/
  graph.rs       — ESNode, ESEdge, ESGraph, ESValue types and operations
  parser.rs      — Edgescript text → ESGraph
  serializer.rs  — ESGraph → Edgescript text
  query.rs       — follow(), incoming() graph traversal
  signal.rs      — EventSignal, propagation with location/faction/region cohesion
  agent.rs       — player agent, context builder, Ollama integration
  npc_agent.rs   — NPC agent, signal-driven reactions
  server.rs      — WebSocket server, broadcast channel
  state.rs       — shared AppState (graph + db + broadcast)
  db.rs          — redb persistence layer
  main.rs        — world initialization, server startup

visualizer/
  index.html     — D3 force graph, live signal animation, node inspector
```

## Signal propagation

When a player acts, a signal fires from their node outward. It reaches neighbors through four channels, each with different affinity:

- Direct edges → affinity defined on the edge (strongest)
- Same location → 0.7 affinity
- Same faction → 0.5 affinity  
- Same region → 0.3 affinity

Nodes absorb signals above their threshold and update their activation. NPC nodes above their `reaction_threshold` spawn an NPC agent call. Signals decay by `0.7` per hop and dissipate below `0.05`.

## Namespace system

```
world/          → shared reality, visible to all agents and propagation
inventory/{player}/   → player's owned items
abilities/{player}/   → player's learned skills
quests/{player}/      → player's active quests
memory/{npc}/         → NPC's private memory (written only by NPC agent)
```

The player agent can only write to world nodes it directly affects and the player's own namespaces. Attempts to write to NPC memory or other players' namespaces are rejected at merge time.

## AI integration

Uses Ollama for local inference. The player agent uses a larger model for narrative quality, NPC agents use a smaller faster model for routine reactions.

```rust
// agent.rs
const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const PLAYER_MODEL: &str = "llama3.1:8b-instruct-q8_0";

// npc_agent.rs
const NPC_MODEL: &str = "phi3:mini";
```

The context builder assembles player state, inventory, abilities, and nearby world nodes into a prompt. The model returns an Edgescript patch that gets validated, namespace-checked, and merged into the live graph. The world is then broadcast to all connected visualizer clients via WebSocket.

## Visualizer

A D3 force graph at `http://localhost:3000` that renders the world graph in real time. Nodes are colored by type. Click any node to inspect its properties. Select a player node and fire a player action — watch the agent think, the patch merge, and signals ripple outward through the graph.

Signal hops animate as pulses traveling along edges. Green pulse = absorbed. Red pulse = ignored. Node activation updates live in the inspector panel.

## Getting started

**Requirements**
- Rust 1.75+
- Ollama running locally with `llama3.1:8b-instruct-q8_0` and `phi3:mini` pulled

```bash
# install and run Ollama
curl -fsSL https://ollama.ai/install.sh | sh
ollama pull llama3.1:8b-instruct-q8_0
ollama pull phi3:mini

# clone and run
git clone https://github.com/AndrewBazen/world-engine
cd world-engine
cargo run
```

Open `http://localhost:3000` in your browser. The world initializes from the hardcoded definition in `main.rs` on first run and persists to `data/world.db` via redb on subsequent runs.

## Running tests

```bash
cargo test -- --nocapture
```

## Roadmap

- [ ] NPC agent — signal-driven independent reactions
- [ ] Two-phase resolution — attempt → success/fail → consequences  
- [ ] Node and edge destruction — marking entities as destroyed
- [ ] Item lifecycle — world items, ownership transfer, drop/claim
- [ ] Multiplayer — contested zones, arbitration, ripple propagation across players
- [ ] Outcome logging — training data accumulation for future fine-tuning
- [ ] Godot 3D frontend — engine as a renderer on top of the graph backend
- [ ] Fine-tuned local models — replace generic LLM calls with world-specific models

## Design philosophy

The graph is always current state. Signals are transient energy, not persistent nodes. Agents only know what they're supposed to know. The world reacts emergently from topology — not from scripted consequence chains. A player's legend is encoded in the state of every node their actions have touched.