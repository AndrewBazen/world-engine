use crate::graph::{ESGraph, ESNode, ESValue};
use serde::{Serialize, Deserialize};

pub const AWARENESS_HALF_LIFE_TURNS: f64 = 5.0;
pub const PROFICIENCY_BONUS: i32 = 3;
pub const BASE_HIT_POINTS: i32 = 8;

/// How good someone is at something.  Categorical rather than numeric
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade { Feeble, Poor, Average, Capable, Exceptional }

impl Grade {
    pub fn value(self) -> i32 {
        match self {
            Grade::Feeble => 6,
            Grade::Poor => 8,
            Grade::Average => 10,
            Grade::Capable => 14,
            Grade::Exceptional => 18,
        }
    }

    pub fn parse(raw: &str) -> Option<Grade> {
        match raw.trim().to_lowercase().as_str() {
            "feeble" => Some(Grade::Feeble),
            "poor" => Some(Grade::Poor),
            "average" => Some(Grade::Average),
            "capable" => Some(Grade::Capable),
            "exceptional" => Some(Grade::Exceptional),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skill {
    Athletics, Acrobatics, Stealth, Perception, Insight,
    Persuasion, Deception, Intimidation, SleightOfHand, Investigation,
}

impl Skill {
    pub fn parse(raw: &str) -> Option<Skill> {
        match raw.trim().to_lowercase().replace(' ', "_").as_str() {
            "athletics" => Some(Skill::Athletics),
            "acrobatics" => Some(Skill::Acrobatics),
            "stealth" => Some(Skill::Stealth),
            "perception" => Some(Skill::Perception),
            "insight" => Some(Skill::Insight),
            "persuasion" => Some(Skill::Persuasion),
            "deception" => Some(Skill::Deception),
            "intimidation" => Some(Skill::Intimidation),
            "sleight_of_hand" => Some(Skill::SleightOfHand),
            "investigation" => Some(Skill::Investigation),
            _ => None,
        }
    }

    fn add_to(self, skills: &mut SkillBlock, bonus: i32) {
        match self {
            Skill::Athletics => skills.athletics += bonus,
            Skill::Acrobatics => skills.acrobatics += bonus,
            Skill::Stealth => skills.stealth += bonus,
            Skill::Perception => skills.perception += bonus,
            Skill::Insight => skills.insight += bonus,
            Skill::Persuasion => skills.persuasion += bonus,
            Skill::Deception => skills.deception += bonus,
            Skill::Intimidation => skills.intimidation += bonus,
            Skill::SleightOfHand => skills.sleight_of_hand += bonus,
            Skill::Investigation => skills.investigation += bonus,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Grades {
    pub physique: Grade,
    pub agility: Grade,
    pub awareness: Grade,
    pub presence: Grade,
}

impl Grades {
    pub fn default() -> Self {
        Grades {
            physique: Grade::Average,
            agility: Grade::Average,
            awareness: Grade::Average,
            presence: Grade::Average,
        }
    }
}

/// The five fields an agent may write when a person first enters the world —
/// and may never write again.
pub const GRADE_FIELDS: [&str; 5] =
    ["physique", "agility", "awareness", "presence", "proficient"];

/// Remove the grade fields from a patch node. Returns true if any were present.
///
/// Grades are identity, not state. Both context builders print them, so a model
/// will happily echo them back — or quietly upgrade itself after a fright. The
/// prompts say not to; this is what makes it true.
pub fn strip_grade_fields(node: &mut ESNode) -> bool {
    let mut found = false;
    for field in GRADE_FIELDS {
        if node.props.remove(field).is_some() {
            found = true;
        }
    }
    found
}

/// Read one grade off a node, defaulting to Average and recording why.
fn read_grade(node: &ESNode, field: &str, complaints: &mut Vec<String>) -> Grade {
    match node.props.get(field) {
        Some(ESValue::Text(raw)) => Grade::parse(raw).unwrap_or_else(|| {
            complaints.push(format!("{}: unknown grade '{}'", field, raw));
            Grade::Average
        }),
        _ => {
            complaints.push(format!("{}: missing, defaulting to average", field));
            Grade::Average
        }
    }
}

/// Read the five emitted fields off a node.
///
/// Returns whatever it understood plus every complaint. A missing or
/// unrecognised grade falls back to Average — but never silently, because a
/// generic stat block nobody chose is the failure the keyword matcher hid.
pub fn read_grades(node: &ESNode) -> (Grades, Vec<Skill>, Vec<String>) {
    let mut complaints = Vec::new();

    let grades = Grades {
        physique:  read_grade(node, "physique",  &mut complaints),
        agility:   read_grade(node, "agility",   &mut complaints),
        awareness: read_grade(node, "awareness", &mut complaints),
        presence:  read_grade(node, "presence",  &mut complaints),
    };

    let mut proficient = Vec::new();
    if let Some(ESValue::Text(list)) = node.props.get("proficient") {
        for name in list.split(',') {
            match Skill::parse(name) {
                Some(s) => proficient.push(s),
                None => complaints.push(format!("proficient: unknown skill '{}'", name.trim())),
            }
        }
    }
    (grades, proficient, complaints)
}

// ── Stat block structs ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// An all-Average NPC with no proficiencies. Delegates rather than
    /// restating the numbers, so retuning `Grade::value()` cannot leave the
    /// default disagreeing with everything else.
    pub fn default() -> Self {
        stat_block_from(&Grades::default(), &[])
    }
}

/// Build a complete stat block from the five emitted fields.
///
/// Pure: takes inputs, returns a fresh block. The old `clamp()` mutated in
/// place and re-applied the constitution modifier on every call, so calling it
/// twice changed the hit points.
pub fn stat_block_from(grades: &Grades, proficient: &[Skill]) -> StatBlock {
    let strength     = grades.physique.value();
    let constitution = grades.physique.value();
    let dexterity    = grades.agility.value();
    let wisdom       = grades.awareness.value();
    let charisma     = grades.presence.value();
    // No grade owns intelligence — nothing consumes it yet, and folding it
    // into awareness would re-couple clever with observant.
    let intelligence = 10;

    let mut skills = SkillBlock::default();
    for s in proficient {
        s.add_to(&mut skills, PROFICIENCY_BONUS);
    }

    let wis_mod = (wisdom - 10) / 2;
    let int_mod = (intelligence - 10) / 2;
    let con_mod = (constitution - 10) / 2;
    let dex_mod = (dexterity - 10) / 2;

    StatBlock {
        strength, dexterity, constitution, intelligence, wisdom, charisma,
        hit_points: (BASE_HIT_POINTS + con_mod).max(1),
        armor_class: 10 + dex_mod,
        speed: 30,
        passive_perception:    10 + wis_mod + skills.perception,
        passive_investigation: 10 + int_mod + skills.investigation,
        passive_insight:       10 + wis_mod + skills.insight,
        skills,
    }
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

    // Mirror the derived sensitivity onto the NPC node itself. Stat blocks live
    // in a private namespace and never reach the browser, so without this the
    // visualizer had to hardcode a baseline and guess.
    let baseline = perception_from_passive(stats.passive_perception as f64);
    if let Some(npc) = graph.get_mut("world", "npc", npc_id) {
        npc.props.insert(
            "awareness_baseline".to_string(),
            ESValue::Number(baseline),
        );
    }
}

pub fn refresh_stat_block(graph: &mut ESGraph, npc_key: &str) -> Vec<String> {
    let node = match graph.nodes.get(npc_key) {
        Some(n) => n.clone(),
        None => return vec![format!("{} does not exist", npc_key)],
    };
    let npc_id = npc_key.split(':').nth(1).unwrap_or(npc_key).to_string();

    let (grades, proficient, complaints) = read_grades(&node);
    let block = stat_block_from(&grades, &proficient);
    write_stat_block(graph, &npc_id, &block);

    complaints
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

/// The single definition of how passive_perception maps to a 0..1 sensitivity.
/// 10 (average) → 0.50, 14 (trained) → 0.70, 5 (poor) → 0.25.
/// The signal threshold is `1.0 - this`, so an average NPC notices anything
/// arriving at strength 0.5 or above.
pub fn perception_from_passive(passive: f64) -> f64 {
    (passive / 20.0).clamp(0.05, 0.95)
}

pub fn get_baseline_awareness(node: &ESNode, graph: &ESGraph) -> f64 {
    perception_from_passive(get_passive(graph, &node.id, "passive_perception"))
}

pub fn current_awareness(node: &ESNode, graph: &ESGraph, turn: u64) -> f64 {
    let baseline = get_baseline_awareness(node, graph);
    let peak = node.get_number("awareness_peak").unwrap_or(baseline);
    let last_raised = node.get_number("awareness_last_raised").unwrap_or(0.0);

    let elapsed = (turn as f64 - last_raised).max(0.0);
    let half_life = node.get_number("awareness_half_life").unwrap_or(AWARENESS_HALF_LIFE_TURNS);
    let decay_rate = std::f64::consts::LN_2 / half_life.max(0.0001);
    let decayed = (peak - baseline) * (-decay_rate * elapsed).exp();

    (baseline + decayed).clamp(0.0, 1.0)
}

/// What this NPC can currently detect. Driven by senses and alertness only —
/// intelligence deliberately does not enter here. A clever NPC is not
/// automatically an observant one.
pub fn current_perception(node: &ESNode, graph: &ESGraph, turn: u64) -> f64 {
    current_awareness(node, graph, turn)
}