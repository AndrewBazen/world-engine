export const WS_URL = 'ws://localhost:3000/ws';
export const NODE_COLORS = {
  player: '#7c5cbf',
  npc:    '#1d9e75',
  item:   '#ba7517',
  scene:  '#d85a30',
};
export const nodeColor = t => NODE_COLORS[t] || '#6b8394';
export const NODE_RADIUS = 10;
export const SIGNAL_DURATION = 300; // ms per hop