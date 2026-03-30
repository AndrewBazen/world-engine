/* global d3 */

function getAwareness(node) {
    const props = node.props || {};
    const peak = props.awareness_peak ?? 0;
    const lastRaised = props.awareness_last_raised ?? 0;
    const baseline = 0.3; // safe default for visualizer
    const decayRate = 0.005;

    if (peak <= baseline || lastRaised === 0) return 0;

    const now = Date.now() / 1000; // seconds since epoch, matching Rust
    const elapsed = now - lastRaised;
    const decayed = (peak - baseline) * Math.exp(-decayRate * elapsed);

    return baseline + decayed;
}

export function startAwarenessLoop(nodesG) {
    function update() {
        nodesG.selectAll('.node-group')
            .filter(d => d.node_type === 'npc')
            .select('circle')
            .each(function(d) {
                const awareness = getAwareness(d);
                const intensity = Math.max(0, awareness - 0.3) / 0.7; // 0 to 1

                d3.select(this)
                    .attr('filter', intensity > 0.01 ? 'url(#glow)' : null)
                    .attr('stroke-width', 1.5 + intensity * 3)
                    .attr('stroke-opacity', 0.5 + intensity * 0.5);
            });

        requestAnimationFrame(update);
    }
    requestAnimationFrame(update);
}