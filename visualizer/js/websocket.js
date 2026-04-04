import { render, nodes, edges } from "./graph.js";
import { animateHop, logHop } from "./signal.js";
import { selectNode, getSelectedNode } from "./panel.js";
import { WS_URL } from "./config.js";
import { renderNodeDetail } from "./detail.js";

let ws = null;

export function connect() {

	ws = new WebSocket(WS_URL);

	ws.onopen = () => {
		document.getElementById('status-dot').classList.add('connected');
		document.getElementById('status-text').textContent = 'connected';
	};

	ws.onclose = () => {
		document.getElementById('status-dot').classList.remove('connected');
		document.getElementById('status-text').textContent = 'reconnecting...';
		setTimeout(connect, 2000);
	};

	ws.onerror = () => {
		document.getElementById('status-text').textContent = 'error';
	};

	ws.onmessage = e => {
		const msg = JSON.parse(e.data);

		if (msg.type === 'snapshot') {
			// build nodes and edges from snapshot
			const nodeMap = {};
			const newNodeArray = msg.nodes.map(n => {
				const existing = nodes.find(x => x.id === n.id);
				const node = existing
					? Object.assign(existing, n)
					: { ...n, x: Math.random() * 800 + 100, y: Math.random() * 600 + 100 };
				nodeMap[n.id] = node;
				return node;
			});

			nodes.length = 0;
			nodes.push(...newNodeArray);

			const newEdgeArray = msg.edges.map(e => ({
				source: e.source,
				target: e.target,
				label:  e.label,
				affinity: e.affinity,
			}));

			edges.length = 0;
			edges.push(...newEdgeArray);

			render();
		}



		if (msg.type === 'signal_hop') {
			animateHop(msg.from, msg.to, msg.absorbed, msg.ambient);
			logHop(msg);
		}

		if (msg.type === 'node_update') {
			// update node props in our local array
			const node = nodes.find(n => n.id === msg.id);
			if (node) {
				node.props = msg.props;
				// if this node is currently selected, refresh the panel
				let selectedNode = getSelectedNode();
				if (selectedNode && selectedNode.id === msg.id) {
						selectNode(node);
				}
			}
		}

		if (msg.type === 'node_detail') {
			renderNodeDetail(msg);
		}
	};
}

export function requestNodeDetail(nodeId) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify({
        type: 'node_detail',
        node_id: nodeId,
    }));
}

function handlePlayerAction() {
    const input = document.getElementById('action-input');
    const context = input.value.trim();
    if (!context || !ws || ws.readyState !== WebSocket.OPEN) return;

    ws.send(JSON.stringify({
        type: 'player_action',
        player_id: 'player:andrew',
        context,
        strength: 0.8,
    }));

    input.value = '';
}

document.getElementById('action-btn').addEventListener('click', handlePlayerAction);
document.getElementById('action-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') handlePlayerAction();
});

function handleFireSignal() {
	// ── Fire signal ───────────────────────────────────────────────────────────────
	let selectedNode = getSelectedNode();
	if (!selectedNode || !ws || ws.readyState !== WebSocket.OPEN) return;

	const context  = document.getElementById('signal-context').value.trim();
	const strength = parseFloat(document.getElementById('strength-slider').value);
	if (!context) { document.getElementById('signal-context').focus(); return; }

	if (selectedNode.node_type === 'player') {
			// player action — goes through AI agent
			ws.send(JSON.stringify({
					type: 'player_action',
					player_id: selectedNode.id,
					context,
					strength,
			}));
	} else {
			// non-player node — raw signal
			ws.send(JSON.stringify({
					type: 'trigger_signal',
					origin_id: selectedNode.id,
					strength,
					context,
			}));
	}
	document.getElementById('signal-context').value = '';
}

document.getElementById('fire-btn').addEventListener('click', handleFireSignal);