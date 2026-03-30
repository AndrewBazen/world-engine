import { nodeColor } from "./config.js";

let selectedNode = null;

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
  
	Object.entries(props).forEach(([k, v]) => {
	  const row = document.createElement('div');
	  row.className = 'prop-row';
	  let valClass = 'prop-val';
	  let display = v;
	  if (typeof v === 'number') { valClass += ' number'; display = v.toFixed(3); }
	  else if (v === true)  { valClass += ' bool-true';  display = 'true'; }
	  else if (v === false) { valClass += ' bool-false'; display = 'false'; }
	  row.innerHTML = `<span class="prop-key">${k}</span><span class="${valClass}">${display}</span>`;
	  list.appendChild(row);
	});
}
  
export function closePanel() {
	document.getElementById('panel').classList.remove('open');
	document.getElementById('log').classList.add('panel-closed');
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
  

