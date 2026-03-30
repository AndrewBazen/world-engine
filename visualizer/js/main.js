import { connect } from './websocket.js';
import { startAwarenessLoop } from './awareness.js';
import { nodesG } from './graph.js';

connect();
startAwarenessLoop(nodesG);