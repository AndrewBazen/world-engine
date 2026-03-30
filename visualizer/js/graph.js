import { NODE_RADIUS, nodeColor } from "./config.js";
import { closePanel, selectNode } from "./panel.js"
/* global d3 */

let simulation, svg, zoomGroup;
export let nodes = [], edges = [];

// ── D3 Setup ─────────────────────────────────────────────────────────────────
const svgEl = document.getElementById('graph-svg');
svg = d3.select(svgEl);

const zoom = d3.zoom()
    .scaleExtent([0.2, 4])
    .on('zoom', e => zoomGroup.attr('transform', e.transform));

svg.call(zoom);
zoomGroup = svg.select('#zoom-group');  

export const edgesG  = zoomGroup.select('#edges-g');
export const nodesG  = zoomGroup.select('#nodes-g');
const labelsG = zoomGroup.select('#labels-g');
export const effectsG = zoomGroup.select('#effects-g');

// ── Simulation ───────────────────────────────────────────────────────────────
function initSimulation(w, h) {
	simulation = d3.forceSimulation()
		.force('link', d3.forceLink().id(d => d.id).distance(120).strength(0.4))
		.force('charge', d3.forceManyBody().strength(-320))
		.force('center', d3.forceCenter(w / 2, h / 2))
		.force('collision', d3.forceCollide(NODE_RADIUS + 12));
}

// ── Render ───────────────────────────────────────────────────────────────────
export function render() {
	const w = svgEl.clientWidth;
	const h = svgEl.clientHeight;
	if (!simulation) initSimulation(w, h);

	// edges
	const edgeSel = edgesG.selectAll('.edge-group')
		.data(edges, d => d.source + '→' + d.target + d.label);

	const edgeEnter = edgeSel.enter().append('g').attr('class', 'edge-group');
	edgeEnter.append('line')
		.attr('class', 'edge-line')
		.attr('marker-end', 'url(#arrowhead)');
	edgeEnter.append('text')
		.attr('class', 'edge-label')
		.attr('text-anchor', 'middle')
		.text(d => d.label);

	edgeSel.exit().remove();

	// nodes
	const nodeSel = nodesG.selectAll('.node-group')
		.data(nodes, d => d.id);

	const nodeEnter = nodeSel.enter().append('g')
		.attr('class', 'node-group')
		.call(d3.drag()
		.on('start', dragStart)
		.on('drag',  dragged)
		.on('end',   dragEnd))
		.on('click', (e, d) => {
		e.stopPropagation();
		selectNode(d);
		});

	nodeEnter.append('circle')
		.attr('class', 'node-circle')
		.attr('r', NODE_RADIUS)
		.attr('fill', d => nodeColor(d.node_type))
		.attr('stroke', d => nodeColor(d.node_type))
		.attr('stroke-width', 1.5)
		.attr('stroke-opacity', 0.5)
		.attr('fill-opacity', 0.85);

	nodeSel.exit().remove();

	// labels
	const labelSel = labelsG.selectAll('.node-label')
		.data(nodes, d => d.id);

	labelSel.enter().append('text')
		.attr('class', 'node-label')
		.attr('text-anchor', 'middle')
		.attr('dy', NODE_RADIUS + 13)
		.text(d => d.id);

	labelSel.exit().remove();

	// update sim
	simulation
		.nodes(nodes)
		.on('tick', ticked);

	simulation.force('link').links(edges);
	simulation.alpha(0.5).restart();

	// update counter
	document.getElementById('node-count').textContent =
		`${nodes.length} nodes · ${edges.length} edges`;

	// click background deselects
	svg.on('click', () => closePanel());
}

function ticked() {
	edgesG.selectAll('.edge-group').each(function(d) {
		const g = d3.select(this);
		const dx = d.target.x - d.source.x;
		const dy = d.target.y - d.source.y;
		const len = Math.sqrt(dx*dx + dy*dy) || 1;
		const tx = d.target.x - (dx/len) * (NODE_RADIUS + 4);
		const ty = d.target.y - (dy/len) * (NODE_RADIUS + 4);

		g.select('line')
			.attr('x1', d.source.x).attr('y1', d.source.y)
			.attr('x2', tx).attr('y2', ty);

		g.select('text')
			.attr('x', (d.source.x + d.target.x) / 2)
			.attr('y', (d.source.y + d.target.y) / 2 - 5);
	});

	nodesG.selectAll('.node-group')
		.attr('transform', d => `translate(${d.x},${d.y})`);

	labelsG.selectAll('.node-label')
		.attr('x', d => d.x)
		.attr('y', d => d.y);
}

// ── Drag ─────────────────────────────────────────────────────────────────────
function dragStart(e, d) {
	if (!e.active) simulation.alphaTarget(0.3).restart();
	d.fx = d.x; d.fy = d.y;
}
function dragged(e, d) { d.fx = e.x; d.fy = e.y; }
function dragEnd(e, d) {
	if (!e.active) simulation.alphaTarget(0);
	d.fx = null; d.fy = null;
}

// ── Init ──────────────────────────────────────────────────────────────────────
window.addEventListener('resize', () => {
	if (simulation) {
	simulation.force('center', d3.forceCenter(
		svgEl.clientWidth / 2,
		svgEl.clientHeight / 2
	));
	simulation.alpha(0.1).restart();
	}
});
  