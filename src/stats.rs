use crate::graph::{ESGraph, ESNode, ESValue};
use serde::{Serialize, Deserialize};

// ── Stat block structs ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatBlock {
    pub strength:     i32,
    pub dexterity:    i32,
    pub constitution: i32,
    pub intelligence: i32,
    pub wisdom:       i32,
    pub charisma:     i32,
    pub hit_points:   i32,
    pub armor_class:  i32,
    pub speed:        i32,
    pub passive_perception:    i32,
    pub passive_investigation: i32,
    pub passive_insight:       i32,
    pub skills: SkillBlock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBlock {
    pub athletics:      i32,
    pub acrobatics:     i32,
    pub stealth:        i32,
    pub perception:     i32,
    pub insight:        i32,
    pub persuasion:     i32,
    pub deception:      i32,
    pub intimidation:   i32,
    pub sleight_of_hand: i32,
    pub investigation:  i32,
}

impl SkillBlock {
    pub fn default() -> Self {
        SkillBlock {
            athletics: 0, acrobatics: 0, stealth: 0,
            perception: 0, insight: 0, persuasion: 0,
            deception: 0, intimidation: 0,
            sleight_of_hand: 0, investigation: 0,
        }
    }
}

impl StatBlock {
    pub fn default() -> Self {
        StatBlock {
            strength: 10, dexterity: 10, constitution: 10,
            intelligence: 10, wisdom: 10, charisma: 10,
            hit_points: 8, armor_class: 10, speed: 30,
            passive_perception: 10,
            passive_investigation: 10,
            passive_insight: 10,
            skills: SkillBlock::default(),
        }
    }

    pub fn clamp(mut self) -> Self {
        self.strength     = self.strength.clamp(1, 20);
        self.dexterity    = self.dexterity.clamp(1, 20);
        self.constitution = self.constitution.clamp(1, 20);
        self.intelligence = self.intelligence.clamp(1, 20);
        self.wisdom       = self.wisdom.clamp(1, 20);
        self.charisma     = self.charisma.clamp(1, 20);
        self.hit_points   = self.hit_points.clamp(1, 999);
        self.armor_class  = self.armor_class.clamp(5, 30);
        self.speed        = self.speed.clamp(0, 60);

        // derive passives from stats and skill bonuses
        let wis_mod = (self.wisdom - 10) / 2;
        let int_mod = (self.intelligence - 10) / 2;
        self.passive_perception    = 10 + wis_mod + self.skills.perception;
        self.passive_investigation = 10 + int_mod + self.skills.investigation;
        self.passive_insight       = 10 + wis_mod + self.skills.insight;

        // derive HP from constitution
        let con_mod = (self.constitution - 10) / 2;
        self.hit_points = (self.hit_points + con_mod).max(1);

        self
    }
}

// ── Helper to read text props ────────────────────────────────

fn get_text(node: &ESNode, key: &str) -> String {
    node.props.get(key)
        .and_then(|v| if let ESValue::Text(s) = v { Some(s.clone()) } else { None })
        .unwrap_or_default()
        .to_lowercase()
}

// ── Rule-based stat generator ────────────────────────────────

pub fn generate_stats(node: &ESNode) -> StatBlock {
    let mut s = StatBlock::default();

    let background     = get_text(node, "background");
    let build          = get_text(node, "build");
    let condition      = get_text(node, "condition");
    let skills         = get_text(node, "notable_skills");
    let weaknesses     = get_text(node, "weaknesses");
    let personality    = get_text(node, "personality");
    let occupation     = get_text(node, "occupation");

    // combine all text for broad keyword matching
    let all = format!("{} {} {} {} {} {} {}",
        background, build, condition, skills, weaknesses, personality, occupation);

    // ── Background / occupation ──────────────────────────────
    if any(&all, &["soldier", "fighter", "warrior", "mercenary", "knight"]) {
        s.strength     += 4;
        s.constitution += 3;
        s.wisdom       += 1;
        s.skills.athletics    += 4;
        s.skills.intimidation += 2;
        s.hit_points   += 10;
        s.armor_class  += 2;
    }
    if any(&all, &["guard", "watchman", "militia", "patrol"]) {
        s.strength     += 2;
        s.constitution += 2;
        s.wisdom       += 2;
        s.skills.perception   += 3;
        s.skills.intimidation += 2;
        s.hit_points   += 6;
    }
    if any(&all, &["thief", "rogue", "criminal", "assassin", "pickpocket"]) {
        s.dexterity    += 4;
        s.intelligence += 2;
        s.skills.stealth          += 4;
        s.skills.sleight_of_hand  += 4;
        s.skills.deception        += 3;
        s.skills.acrobatics       += 2;
    }
    if any(&all, &["merchant", "trader", "shopkeeper", "vendor"]) {
        s.charisma     += 3;
        s.intelligence += 3;
        s.skills.persuasion   += 4;
        s.skills.deception    += 2;
        s.skills.insight      += 2;
    }
    if any(&all, &["farmer", "laborer", "peasant", "worker"]) {
        s.strength     += 3;
        s.constitution += 4;
        s.skills.athletics += 2;
        s.hit_points   += 4;
    }
    if any(&all, &["scholar", "wizard", "mage", "arcanist", "sage"]) {
        s.intelligence += 5;
        s.wisdom       += 3;
        s.skills.investigation += 4;
        s.skills.perception    += 2;
    }
    if any(&all, &["priest", "cleric", "healer", "monk", "acolyte"]) {
        s.wisdom   += 5;
        s.charisma += 2;
        s.skills.insight    += 4;
        s.skills.persuasion += 3;
    }
    if any(&all, &["ranger", "hunter", "scout", "tracker"]) {
        s.dexterity    += 3;
        s.wisdom       += 3;
        s.constitution += 2;
        s.skills.perception   += 4;
        s.skills.stealth      += 3;
        s.skills.athletics    += 2;
    }
    if any(&all, &["bard", "performer", "entertainer", "actor"]) {
        s.charisma     += 4;
        s.dexterity    += 2;
        s.skills.persuasion   += 4;
        s.skills.deception    += 3;
        s.skills.acrobatics   += 2;
    }
    if any(&all, &["blacksmith", "smith", "craftsman", "artisan"]) {
        s.strength     += 3;
        s.constitution += 2;
        s.dexterity    += 2;
        s.skills.athletics += 2;
    }
    if any(&all, &["spy", "agent", "infiltrator", "informant"]) {
        s.dexterity    += 3;
        s.intelligence += 3;
        s.charisma     += 2;
        s.skills.deception        += 4;
        s.skills.stealth          += 3;
        s.skills.insight          += 3;
        s.skills.sleight_of_hand  += 2;
    }
    if any(&all, &["commander", "captain", "officer", "general", "leader"]) {
        s.strength     += 2;
        s.intelligence += 3;
        s.charisma     += 3;
        s.wisdom       += 2;
        s.skills.intimidation += 4;
        s.skills.persuasion   += 3;
        s.skills.insight      += 2;
        s.hit_points   += 8;
    }

    // ── Build ────────────────────────────────────────────────
    if any(&build, &["stocky", "muscular", "powerful", "broad", "brawny"]) {
        s.strength     += 2;
        s.constitution += 1;
    }
    if any(&build, &["lean", "wiry", "agile", "lithe", "slim"]) {
        s.dexterity += 2;
        s.strength  -= 1;
    }
    if any(&build, &["frail", "thin", "gaunt", "weak", "scrawny"]) {
        s.constitution -= 2;
        s.strength     -= 2;
    }
    if any(&build, &["large", "hulking", "imposing", "massive", "giant"]) {
        s.strength  += 3;
        s.charisma  += 1;
        s.hit_points += 4;
    }
    if any(&build, &["small", "short", "slight", "petite"]) {
        s.strength  -= 1;
        s.dexterity += 1;
    }

    // ── Condition / age ──────────────────────────────────────
    if any(&all, &["old", "elderly", "aged", "ancient", "veteran"]) {
        s.strength     -= 2;
        s.dexterity    -= 2;
        s.constitution -= 2;
        s.wisdom       += 3;
        s.intelligence += 1;
    }
    if any(&all, &["young", "youth", "teenage", "adolescent"]) {
        s.constitution += 1;
        s.wisdom       -= 2;
        s.strength     -= 1;
    }
    if any(&condition, &["injured", "wounded", "lame", "crippled"]) {
        s.dexterity -= 3;
        s.speed     -= 10;
        s.hit_points -= 5;
    }
    if any(&condition, &["sick", "diseased", "ill", "weak"]) {
        s.constitution -= 3;
        s.hit_points   -= 5;
        s.strength     -= 2;
    }
    if any(&condition, &["drunk", "intoxicated"]) {
        s.dexterity    -= 3;
        s.intelligence -= 2;
        s.charisma     += 1;
    }
    if any(&condition, &["healthy", "fit", "well"]) {
        s.constitution += 1;
        s.hit_points   += 2;
    }

    // ── Notable skills ───────────────────────────────────────
    if any(&skills, &["perceptive", "sharp eyes", "keen", "observant", "watchful"]) {
        s.wisdom += 2;
        s.skills.perception += 4;
    }
    if any(&skills, &["persuasive", "charming", "silver tongue", "diplomat"]) {
        s.charisma += 3;
        s.skills.persuasion += 4;
    }
    if any(&skills, &["stealthy", "quiet", "sneaky", "shadow"]) {
        s.dexterity += 2;
        s.skills.stealth += 4;
    }
    if any(&skills, &["strong", "powerful", "mighty"]) {
        s.strength += 3;
        s.skills.athletics += 3;
    }
    if any(&skills, &["intelligent", "clever", "brilliant", "smart"]) {
        s.intelligence += 3;
        s.skills.investigation += 3;
    }
    if any(&skills, &["deceptive", "liar", "manipulative"]) {
        s.charisma += 2;
        s.skills.deception += 4;
    }
    if any(&skills, &["intimidating", "fearsome", "threatening"]) {
        s.strength += 1;
        s.charisma += 1;
        s.skills.intimidation += 4;
    }

    // ── Weaknesses ───────────────────────────────────────────
    if any(&weaknesses, &["slow", "heavy", "lumbering", "clumsy"]) {
        s.dexterity -= 2;
        s.speed     -= 5;
    }
    if any(&weaknesses, &["trusting", "naive", "gullible"]) {
        s.wisdom -= 2;
        s.skills.insight -= 3;
    }
    if any(&weaknesses, &["coward", "fearful", "timid"]) {
        s.charisma -= 2;
        s.skills.intimidation -= 3;
    }
    if any(&weaknesses, &["arrogant", "reckless", "impulsive"]) {
        s.wisdom -= 2;
    }
    if any(&weaknesses, &["blind", "poor vision", "short sighted"]) {
        s.skills.perception -= 4;
        s.wisdom -= 1;
    }
    if any(&weaknesses, &["deaf", "hard of hearing"]) {
        s.skills.perception -= 2;
        s.wisdom -= 1;
    }

    // ── Personality ──────────────────────────────────────────
    if any(&personality, &["vigilant", "alert", "watchful"]) {
        s.wisdom += 1;
        s.skills.perception += 2;
    }
    if any(&personality, &["paranoid", "suspicious"]) {
        s.wisdom += 1;
        s.skills.perception  += 3;
        s.skills.insight     += 2;
    }
    if any(&personality, &["cautious", "careful"]) {
        s.wisdom       += 1;
        s.intelligence += 1;
    }
    if any(&personality, &["reckless", "bold", "brash"]) {
        s.strength += 1;
        s.wisdom   -= 1;
    }
    if any(&personality, &["cunning", "scheming", "devious"]) {
        s.intelligence += 2;
        s.skills.deception   += 2;
        s.skills.insight     += 1;
    }
    if any(&personality, &["simple", "dim", "oblivious"]) {
        s.intelligence -= 2;
        s.wisdom       -= 1;
        s.skills.perception -= 2;
    }

    s.clamp()
}

// ── Helper — any keyword match ───────────────────────────────

fn any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

// ── Write stat block to graph ────────────────────────────────

pub fn write_stat_block(graph: &mut ESGraph, npc_id: &str, stats: &StatBlock) {
    let mut node = ESNode::new(
        &format!("stats/{}", npc_id),
        "stats",
        "block",
    );

    // core attributes
    node.props.insert("strength".to_string(),     ESValue::Number(stats.strength as f64));
    node.props.insert("dexterity".to_string(),    ESValue::Number(stats.dexterity as f64));
    node.props.insert("constitution".to_string(), ESValue::Number(stats.constitution as f64));
    node.props.insert("intelligence".to_string(), ESValue::Number(stats.intelligence as f64));
    node.props.insert("wisdom".to_string(),       ESValue::Number(stats.wisdom as f64));
    node.props.insert("charisma".to_string(),     ESValue::Number(stats.charisma as f64));

    // combat derived
    node.props.insert("hit_points".to_string(),  ESValue::Number(stats.hit_points as f64));
    node.props.insert("max_hp".to_string(),      ESValue::Number(stats.hit_points as f64));
    node.props.insert("armor_class".to_string(), ESValue::Number(stats.armor_class as f64));
    node.props.insert("speed".to_string(),       ESValue::Number(stats.speed as f64));

    // perception derived
    node.props.insert("passive_perception".to_string(),
        ESValue::Number(stats.passive_perception as f64));
    node.props.insert("passive_investigation".to_string(),
        ESValue::Number(stats.passive_investigation as f64));
    node.props.insert("passive_insight".to_string(),
        ESValue::Number(stats.passive_insight as f64));

    // skills
    node.props.insert("athletics".to_string(),
        ESValue::Number(stats.skills.athletics as f64));
    node.props.insert("acrobatics".to_string(),
        ESValue::Number(stats.skills.acrobatics as f64));
    node.props.insert("stealth".to_string(),
        ESValue::Number(stats.skills.stealth as f64));
    node.props.insert("perception".to_string(),
        ESValue::Number(stats.skills.perception as f64));
    node.props.insert("insight".to_string(),
        ESValue::Number(stats.skills.insight as f64));
    node.props.insert("persuasion".to_string(),
        ESValue::Number(stats.skills.persuasion as f64));
    node.props.insert("deception".to_string(),
        ESValue::Number(stats.skills.deception as f64));
    node.props.insert("intimidation".to_string(),
        ESValue::Number(stats.skills.intimidation as f64));
    node.props.insert("sleight_of_hand".to_string(),
        ESValue::Number(stats.skills.sleight_of_hand as f64));
    node.props.insert("investigation".to_string(),
        ESValue::Number(stats.skills.investigation as f64));

    graph.insert(node);
}

// ── Read helpers ─────────────────────────────────────────────

pub fn get_stat_block<'a>(graph: &'a ESGraph, npc_id: &str) -> Option<&'a ESNode> {
    let key = format!("stats/{}/stats:block", npc_id);
    graph.nodes.get(&key)
}

pub fn get_stat(graph: &ESGraph, npc_id: &str, stat: &str) -> f64 {
    get_stat_block(graph, npc_id)
        .and_then(|n: &ESNode| n.get_number(stat))
        .unwrap_or(10.0)
}

pub fn get_passive(graph: &ESGraph, npc_id: &str, passive: &str) -> f64 {
    get_stat_block(graph, npc_id)
        .and_then(|n: &ESNode| n.get_number(passive))
        .unwrap_or(10.0)
}

pub fn get_skill(graph: &ESGraph, npc_id: &str, skill: &str) -> f64 {
    get_stat_block(graph, npc_id)
        .and_then(|n: &ESNode| n.get_number(skill))
        .unwrap_or(0.0)
}

pub fn has_stat_block(graph: &ESGraph, npc_id: &str) -> bool {
    let key = format!("stats/{}/stats:block", npc_id);
    graph.nodes.contains_key(&key)
}

// ── Awareness / perception ────────────────────────────────────

pub fn get_baseline_awareness(node: &ESNode, graph: &ESGraph) -> f64 {
    let passive = get_passive(graph, &node.id, "passive_perception");
    ((passive - 1.0) / 29.0).clamp(0.1, 0.95)
}

pub fn get_intelligence_ceiling(node: &ESNode, graph: &ESGraph) -> f64 {
    let intelligence = get_stat(graph, &node.id, "intelligence");
    ((intelligence - 1.0) / 19.0).clamp(0.1, 1.0)
}

pub fn current_awareness(node: &ESNode, graph: &ESGraph) -> f64 {
    let baseline = get_baseline_awareness(node, graph);
    let peak = node.get_number("awareness_peak").unwrap_or(baseline);
    let last_raised = node.get_number("awareness_last_raised").unwrap_or(0.0);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let elapsed = now - last_raised;
    let decay_rate = node.get_number("awareness_decay_rate").unwrap_or(0.005);
    let decayed = (peak - baseline) * (-decay_rate * elapsed).exp();

    (baseline + decayed).clamp(0.0, 1.0)
}

pub fn current_perception(node: &ESNode, graph: &ESGraph) -> f64 {
    let awareness = current_awareness(node, graph);
    let ceiling = get_intelligence_ceiling(node, graph);
    awareness.max(ceiling)
}