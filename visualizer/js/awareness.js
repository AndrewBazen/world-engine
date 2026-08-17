/* global d3 */
import { DEFAULT_AWARENESS_BASELINE, AWARENESS_DECAY_RATE } from './config.js';

// An NPC's resting sensitivity, derived server-side from passive_perception
// and stamped onto the node. This used to be hardcoded at 0.3 here, which
// drifted the moment the perception curve changed.
export function baselineOf(node) {
    const b = (node.props || {}).awareness_baseline;
    return typeof b === 'number' ? b : DEFAULT_AWARENESS_BASELINE;
}

// Mirrors stats::current_awareness — a peak above baseline decaying back down.
export function getAwareness(node) {
    const props = node.props || {};
    const baseline = baselineOf(node);
    const peak = props.awareness_peak;
    const lastRaised = props.awareness_last_raised;

    if (typeof peak !== 'number' || typeof lastRaised !== 'number') return baseline;
    if (peak <= baseline) return baseline;

    const now = Date.now() / 1000; // seconds since epoch, matching Rust
    const elapsed = Math.max(0, now - lastRaised);
    const decayed = (peak - baseline) * Math.exp(-AWARENESS_DECAY_RATE * elapsed);

    return Math.min(1, baseline + decayed);
}

// The signal strength this NPC needs before it notices anything.
export function thresholdOf(node) {
    return Math.max(0, 1 - getAwareness(node));
}

// 0 at rest, 1 when fully alert — scaled against this NPC's own baseline
// rather than a shared constant, so a sharp NPC and a dull one both read.
export function alertness(node) {
    const baseline = baselineOf(node);
    const headroom = Math.max(0.0001, 1 - baseline);
    return Math.max(0, Math.min(1, (getAwareness(node) - baseline) / headroom));
}

export function startAwarenessLoop(nodesG) {
    function update() {
        nodesG.selectAll('.node-group')
            .filter(d => d.node_type === 'npc')
            .select('circle')
            .each(function (d) {
                const intensity = alertness(d);

                d3.select(this)
                    .attr('filter', intensity > 0.01 ? 'url(#glow)' : null)
                    .attr('stroke-width', 1.5 + intensity * 3)
                    .attr('stroke-opacity', 0.5 + intensity * 0.5);
            });

        requestAnimationFrame(update);
    }
    requestAnimationFrame(update);
}
