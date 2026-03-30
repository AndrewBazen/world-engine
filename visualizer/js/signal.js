import { NODE_RADIUS, SIGNAL_DURATION } from "./config.js";
import { nodes, nodesG, edgesG, effectsG } from "./graph.js";
/* global d3 */

// ── Signal animation ──────────────────────────────────────────────────────────
export function animateHop(fromId, toId, absorbed, ambient) {
	const fromNode = nodes.find(n => n.id === fromId);
	const toNode   = nodes.find(n => n.id === toId);

	if (!fromNode || !toNode) return;
  
	const color = absorbed ? '#39ff8a' : '#ff4444';

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

			if (absorbed) {
				nodesG.selectAll('.node-group')
					.filter(d => d.id === toId)
					.select('circle')
					.transition().delay(delay)
					.duration(200)
					.attr('fill-opacity', 1)
					.attr('stroke-opacity', 1)
					.transition().duration(400)
					.attr('fill-opacity', 0.85)
					.attr('stroke-opacity', 0.5);
			}
		// target node lights up if absorbed
	} else {
		// traveling pulse
		const pulse = effectsG.append('circle')
			.attr('class', 'signal-pulse')
			.attr('r', 4)
			.attr('fill', color)
			.attr('opacity', 0.9)
			.attr('cx', fromNode.x)
			.attr('cy', fromNode.y);

		pulse.transition()
			.duration(SIGNAL_DURATION)
			.ease(d3.easeLinear)
			.attr('cx', toNode.x)
			.attr('cy', toNode.y)
			.on('end', () => {
				pulse.remove();

				if (absorbed) {
					// absorption ring
					effectsG.append('circle')
						.attr('class', 'absorb-ring')
						.attr('cx', toNode.x).attr('cy', toNode.y)
						.attr('r', NODE_RADIUS)
						.attr('stroke', color)
						.on('animationend', function() { d3.select(this).remove(); });

					// briefly brighten target node
					nodesG.selectAll('.node-group')
						.filter(d => d.id === toId)
						.select('circle')
						.transition().duration(200)
						.attr('fill-opacity', 1)
						.attr('stroke-opacity', 1)
						.transition().duration(400)
						.attr('fill-opacity', 0.85)
						.attr('stroke-opacity', 0.5);
				} else {
					// dim flash on ignore
					nodesG.selectAll('.node-group')
						.filter(d => d.id === toId)
						.select('circle')
						.transition().duration(150)
						.attr('fill-opacity', 0.2)
						.transition().duration(150)
						.attr('fill-opacity', 0.85);
				}
			}
		);

		// highlight the edge
		edgesG.selectAll('.edge-group')
			.filter(d => d.source.id === fromId && d.target.id === toId)
			.select('line')
			.transition().duration(SIGNAL_DURATION)
			.attr('stroke', color)
			.attr('stroke-opacity', 0.8)
			.attr('stroke-width', 2)
			.transition().duration(600)
			.attr('stroke', 'rgba(255,255,255,0.08)')
			.attr('stroke-opacity', 1)
			.attr('stroke-width', 1);
	}
}

  
// ── Signal log ────────────────────────────────────────────────────────────────
export function logHop(msg) {
	const entries = document.getElementById('log-entries');
	const el = document.createElement('div');
	el.className = 'log-entry';
	const time = new Date().toLocaleTimeString('en', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
	el.innerHTML = `
		<span class="log-time">${time}</span>
		<span class="log-from">${msg.from}</span>
		<span style="color:var(--text2)">→</span>
		<span class="log-to">${msg.to}</span>
		<span class="log-strength">${msg.strength.toFixed(2)}</span>
		<span class="log-absorbed ${msg.absorbed ? 'yes' : 'no'}">${msg.absorbed ? 'absorbed' : 'ignored'}</span>
		<span class="log-context">${msg.context}</span>
		`;
	entries.prepend(el);
	// keep log bounded
	while (entries.children.length > 40) entries.removeChild(entries.lastChild);
}