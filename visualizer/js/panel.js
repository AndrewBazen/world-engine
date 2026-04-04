import { nodeColor } from "./config.js";
import { edges } from "./graph.js";
import { requestNodeDetail } from "./websocket.js";

let selectedNode = null;

const NPC_SECTIONS = {
    'Identity': ['narrative', 'occupation', 'personality', 'disposition', 'background', 'build', 'condition', 'weaknesses', 'notable_skills'],
    'State': ['alert_level', 'current_action', 'location', 'region'],
    'Awareness': ['awareness_peak', 'awareness_last_raised', 'last_signal_context', 'last_signal_strength'],
};

const PLAYER_SECTIONS = {
    'Identity': ['narrative', 'class', 'race', 'dominant_trait'],
    'State': ['location', 'region', 'gold', 'notable_actions', 'courage'],
};


function createPropRow(k, v) {
	const row = document.createElement('div');
	row.className = 'prop-row';
	let valClass = 'prop-val';
	let display = v;
	if (typeof v === 'number') { valClass += ' number'; display = v.toFixed(3); }
	else if (v === true) { valClass += ' bool-true'; display = 'true'; }
	else if (v === false) { valClass += ' bool-false'; display = 'false'; }
	row.innerHTML = `<span class="prop-key">${k}</span><span class="${valClass}">${display}</span>`;
	return row;
}


// ── Panel ─────────────────────────────────────────────────────────────────────
export function selectNode(d) {
	selectedNode = d;
	const panel = document.getElementById('panel');
	const log   = document.getElementById('log');
	panel.classList.add('open');
	log.classList.remove('panel-closed');
  
	document.getElementById('panel-title').textContent = d.id;
  
	const props = d.props || {};
	const list  = document.getElementById('props-list');
	list.innerHTML = '';
  
	// type badge
	const badge = document.createElement('div');
	badge.className = 'prop-row';
	badge.innerHTML = `<span class="prop-key">type</span><span class="prop-val" style="color:${nodeColor(d.node_type)}">${d.node_type}</span>`;
	list.appendChild(badge);
  
	const sections = d.node_type === 'npc' ? NPC_SECTIONS 
	: d.node_type === 'player' ? PLAYER_SECTIONS 
	: null; 

	if (sections) {
		// track which props have been placed in a section
		const placed = new Set();

		Object.entries(sections).forEach(([sectionName, keys]) => {
			const sectionProps = keys.filter(k => props[k] !== undefined);
			if (sectionProps.length === 0) return;

			// section header
			const header = document.createElement('div');
			header.className = 'section-label';
			header.style.cursor = 'pointer';
			header.style.marginTop = '12px';
			header.textContent = sectionName;
			list.appendChild(header);

			// section content container
			const content = document.createElement('div');
			content.className = 'section-content';

			sectionProps.forEach(k => {
				content.appendChild(createPropRow(k, props[k]));
				placed.add(k);
			});

			list.appendChild(content);

			// toggle collapse on header click
			header.addEventListener('click', () => {
				content.style.display = content.style.display === 'none' ? '' : 'none';
			});
		});

		// "Other" section for uncategorized props
		const remaining = Object.entries(props).filter(([k]) => !placed.has(k));
		if (remaining.length > 0) {
			const header = document.createElement('div');
			header.className = 'section-label';
			header.style.cursor = 'pointer';
			header.style.marginTop = '12px';
			header.textContent = 'Other';
			list.appendChild(header);

			const content = document.createElement('div');
			remaining.forEach(([k, v]) => {
				content.appendChild(createPropRow(k, v));
			});
			list.appendChild(content);

			header.addEventListener('click', () => {
				content.style.display = content.style.display === 'none' ? '' : 'none';
			});
		}

		requestNodeDetail(d.id);
	} else {
		// flat display for locations, factions, etc.
		Object.entries(props).forEach(([k, v]) => {
			list.appendChild(createPropRow(k, v));
		});
	}

	// edges section
	const nodeEdges = edges.filter(e => {
		const sourceId = typeof e.source === 'object' ? e.source.id : e.source;
		return sourceId === d.id;
	});

	if (nodeEdges.length > 0) {
		const header = document.createElement('div');
		header.className = 'section-label';
		header.style.cursor = 'pointer';
		header.style.marginTop = '12px';
		header.textContent = 'Relationships';
		list.appendChild(header);

		const content = document.createElement('div');
		nodeEdges.forEach(e => {
			const targetId = typeof e.target === 'object' ? e.target.id : e.target;
			const row = document.createElement('div');
			row.className = 'prop-row';
			row.innerHTML = `<span class="prop-key">${e.label}</span><span class="prop-val">${targetId}</span>`;
			content.appendChild(row);
		});

		list.appendChild(content);

		header.addEventListener('click', () => {
			content.style.display = content.style.display === 'none' ? '' : 'none';
		});
	}
	document.getElementById('action-bar').classList.remove('panel-closed');
}


  
export function closePanel() {
	document.getElementById('panel').classList.remove('open');
	document.getElementById('log').classList.add('panel-closed');
	document.getElementById('action-bar').classList.add('panel-closed');
	selectedNode = null;
}

export function getSelectedNode() {
	return selectedNode;
}
  
document.getElementById('panel-close').addEventListener('click', closePanel);
  
// ── Strength slider ───────────────────────────────────────────────────────────
document.getElementById('strength-slider').addEventListener('input', function() {
	document.getElementById('strength-val').textContent = parseFloat(this.value).toFixed(2);
});
  

