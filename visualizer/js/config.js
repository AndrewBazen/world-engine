export const WS_URL = 'ws://localhost:3000/ws';

export const NODE_COLORS = {
  player:   '#7c5cbf',
  npc:      '#1d9e75',
  item:     '#ba7517',
  location: '#d85a30',
  scene:    '#d85a30',
  monster:  '#b03a5b',
  faction:  '#4a7ab0',
};
export const nodeColor = t => NODE_COLORS[t] || '#6b8394';

export const NODE_RADIUS = 10;
export const SIGNAL_DURATION = 300; // ms per hop

// Signal hop colours — absorbed, missed the perception check, or just
// passing through a node that cannot perceive at all.
export const HOP_COLORS = {
  absorbed: '#39ff8a',
  ignored:  '#ff4444',
  transit:  '#4d7f99',
};

// Fallback only. The server stamps `awareness_baseline` on every NPC that has
// a stat block; this is what we use before that arrives.
export const DEFAULT_AWARENESS_BASELINE = 0.5;

// Mirrors stats::current_awareness. Kept in sync by the server sending the
// baseline rather than this file guessing at it.
export const AWARENESS_DECAY_RATE = 0.005;
