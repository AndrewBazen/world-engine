import { NODE_RADIUS, SIGNAL_DURATION, HOP_COLORS } from "./config.js";
import { nodes, nodesG, edgesG, effectsG } from "./graph.js";
/* global d3 */

function hopKind(absorbed, transit) {
	if (transit) return 'transit';
	return absorbed ? 'absorbed' : 'ignored';
}

// Brief brighten/dim on the target node.
function flashNode(toId, absorbed, delay = 0) {
	const sel = nodesG.selectAll('.node-group')
		.filter(d => d.id === toId)
		.select('circle');

	if (absorbed) {
		sel.transition().delay(delay).duration(200)
			.attr('fill-opacity', 1)
			.attr('stroke-opacity', 1)
			.transition().duration(400)
			.attr('fill-opacity', 0.85)
			.attr('stroke-opacity', 0.5);
	} else {
		sel.transition().delay(delay).duration(150)
			.attr('fill-opacity', 0.2)
			.transition().duration(150)
			.attr('fill-opacity', 0.85);
	}
}

// ── Signal animation ──────────────────────────────────────────────────────────
// The engine no longer sleeps between rings — it emits the whole propagation at
// once and tags each hop with its distance from the origin. Staggering here
// keeps the ripple readable without stalling the simulation.
export function animateHop(fromId, toId, absorbed, ambient, transit = false, hop = 0) {
	const delay = Math.max(0, hop - 1) * SIGNAL_DURATION;
	if (delay > 0) {
		setTimeout(() => drawHop(fromId, toId, absorbed, ambient, transit), delay);
	} else {
		drawHop(fromId, toId, absorbed, ambient, transit);
	}
}

function drawHop(fromId, toId, absorbed, ambient, transit) {
	const fromNode = nodes.find(n => n.id === fromId);
	const toNode   = nodes.find(n => n.id === toId);

	if (!fromNode || !toNode) return;

	const kind  = hopKind(absorbed, transit);
	const color = HOP_COLORS[kind];

	if (ambient) {
		const dx = toNode.x - fromNode.x;
		const dy = toNode.y - fromNode.y;
		const dist = Math.sqrt(dx * dx + dy * dy);
		const rippleRadius = Math.max(dist + NODE_RADIUS, 80);
		const delay = (dist / rippleRadius) * SIGNAL_DURATION * 2;

		// ripple ring expanding from origin
		const ring = effectsG.append('circle')
			.attr('class', 'ripple-ring')
			.attr('r', NODE_RADIUS)
			.attr('stroke', color)
			.attr('stroke-width', 1.5)
			.attr('fill', 'none')
			.attr('opacity', 0.8)
			.attr('cx', fromNode.x)
			.attr('cy', fromNode.y);

		ring.transition()
			.duration(SIGNAL_DURATION * 2)
			.ease(d3.easeQuadOut)
			.attr('r', rippleRadius)
			.attr('opacity', 0)
			.on('end', () => ring.remove());

		if (absorbed) flashNode(toId, true, delay);
		return;
	}

	// travelling pulse. Transit hops are thinner and dimmer — the signal is
	// crossing a place or an object, not being noticed by anyone.
	const pulse = effectsG.append('circle')
		.attr('class', 'signal-pulse')
		.attr('r', transit ? 2.5 : 4)
		.attr('fill', color)
		.attr('opacity', transit ? 0.5 : 0.9)
		.attr('cx', fromNode.x)
		.attr('cy', fromNode.y);

	pulse.transition()
		.duration(SIGNAL_DURATION)
		.ease(d3.easeLinear)
		.attr('cx', toNode.x)
		.attr('cy', toNode.y)
		.on('end', () => {
			pulse.remove();

			// nothing perceived it — no ring, no flash
			if (transit) return;

			if (absorbed) {
				effectsG.append('circle')
					.attr('class', 'absorb-ring')
					.attr('cx', toNode.x).attr('cy', toNode.y)
					.attr('r', NODE_RADIUS)
					.attr('stroke', color)
					.on('animationend', function () { d3.select(this).remove(); });
			}

			flashNode(toId, absorbed);
		});

	// highlight the edge it travelled along
	edgesG.selectAll('.edge-group')
		.filter(d => d.source.id === fromId && d.target.id === toId)
		.select('line')
		.transition().duration(SIGNAL_DURATION)
		.attr('stroke', color)
		.attr('stroke-opacity', transit ? 0.4 : 0.8)
		.attr('stroke-width', transit ? 1.5 : 2)
		.transition().duration(600)
		.attr('stroke', 'rgba(255,255,255,0.08)')
		.attr('stroke-opacity', 1)
		.attr('stroke-width', 1);
}

// ── Signal log ────────────────────────────────────────────────────────────────
const KIND_LABEL = {
	absorbed: 'absorbed',
	ignored:  'missed',
	transit:  'through',
};

export function logHop(msg) {
	const entries = document.getElementById('log-entries');
	const el = document.createElement('div');
	const kind = hopKind(msg.absorbed, msg.transit);

	el.className = `log-entry ${kind}`;
	const time = new Date().toLocaleTimeString('en', {
		hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit',
	});

	el.innerHTML = `
		<span class="log-time">${time}</span>
		<span class="log-from">${msg.from}</span>
		<span style="color:var(--text2)">${msg.ambient ? '⤳' : '→'}</span>
		<span class="log-to">${msg.to}</span>
		<span class="log-strength">${msg.strength.toFixed(2)}</span>
		<span class="log-absorbed ${kind}">${KIND_LABEL[kind]}</span>
		<span class="log-context">${msg.context}</span>
		`;
	entries.prepend(el);
	// keep log bounded
	while (entries.children.length > 40) entries.removeChild(entries.lastChild);
}
