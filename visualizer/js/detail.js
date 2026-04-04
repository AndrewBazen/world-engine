import { NODE_RADIUS, nodeColor } from './config.js';
/* global d3 */

let simulation = null;

export function renderNodeDetail(msg) {
    const overlay = document.getElementById('detail-overlay');
    const svgEl = document.getElementById('detail-svg');
    
    overlay.classList.add('open');
    document.getElementById('detail-title').textContent = msg.center;

    // clear previous
    const svg = d3.select(svgEl);
    svg.selectAll('*').remove();

    const width = svgEl.clientWidth;
    const height = svgEl.clientHeight;

    const edgesG = svg.append('g');
    const nodesG = svg.append('g');
    const labelsG = svg.append('g');

    // build edges, filtering to only include edges where both source and target exist
    const nodeIds = new Set(msg.nodes.map(n => n.id));
    const edges = msg.edges
        .filter(e => nodeIds.has(e.source) && nodeIds.has(e.target))
        .map(e => ({ ...e }));

    const nodes = msg.nodes.map(n => ({
        ...n,
        x: width / 2 + (Math.random() - 0.5) * 100,
        y: height / 2 + (Math.random() - 0.5) * 100,
    }));

    simulation = d3.forceSimulation(nodes)
        .force('link', d3.forceLink(edges).id(d => d.id).distance(60).strength(0.5))
        .force('charge', d3.forceManyBody().strength(-150))
        .force('center', d3.forceCenter(width / 2, height / 2))
        .force('collision', d3.forceCollide(NODE_RADIUS + 8));

    // render edges
    const edgeSel = edgesG.selectAll('line')
        .data(edges)
        .enter().append('line')
        .attr('stroke', 'rgba(255,255,255,0.12)')
        .attr('stroke-width', 1);

    // render edge labels
    const edgeLabelSel = edgesG.selectAll('text')
        .data(edges)
        .enter().append('text')
        .attr('font-family', 'var(--font-mono)')
        .attr('font-size', '8px')
        .attr('fill', 'rgba(255,255,255,0.25)')
        .attr('text-anchor', 'middle')
        .text(d => d.label);

    // render nodes
    const nodeSel = nodesG.selectAll('circle')
        .data(nodes)
        .enter().append('circle')
        .call(d3.drag()
            .on('start', dragStart)
            .on('drag', dragged)
            .on('end', dragEnd))
        .attr('r', d => d.id === msg.center ? NODE_RADIUS + 2 : NODE_RADIUS - 2)
        .attr('fill', d => nodeColor(d.node_type))
        .attr('fill-opacity', 0.85)
        .attr('stroke', d => d.id === msg.center ? 'var(--accent)' : nodeColor(d.node_type))
        .attr('stroke-width', d => d.id === msg.center ? 2 : 1)
        .attr('stroke-opacity', 0.6);

    // render labels
    const labelSel = labelsG.selectAll('text')
        .data(nodes)
        .enter().append('text')
        .attr('font-family', 'var(--font-mono)')
        .attr('font-size', '9px')
        .attr('fill', 'var(--text2)')
        .attr('text-anchor', 'middle')
        .attr('dy', NODE_RADIUS + 10)
        .text(d => d.id);

    simulation.on('tick', () => {
        edgeSel
            .attr('x1', d => d.source.x).attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x).attr('y2', d => d.target.y);

        edgeLabelSel
            .attr('x', d => (d.source.x + d.target.x) / 2)
            .attr('y', d => (d.source.y + d.target.y) / 2 - 4);

        nodeSel
            .attr('cx', d => d.x).attr('cy', d => d.y);

        labelSel
            .attr('x', d => d.x).attr('y', d => d.y);
    });
}

function dragStart(e, d) {
    if (!e.active) simulation.alphaTarget(0.3).restart();
    d.fx = d.x; d.fy = d.y;
}
function dragged(e, d) { d.fx = e.x; d.fy = e.y; }
function dragEnd(e, d) {
    if (!e.active) simulation.alphaTarget(0);
    d.fx = null; d.fy = null;
}

export function closeDetail() {
    document.getElementById('detail-overlay').classList.remove('open');
    if (simulation) simulation.stop();
}

document.getElementById('detail-close').addEventListener('click', closeDetail);