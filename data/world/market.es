# ── Locations ────────────────────────────────────────────────
@location:market_district
  name: "Market District"
  description: "Crowded stalls, close alleys, and too many eyes."

@location:docks
  name: "The Docks"
  description: "Salt, rope, and cargo nobody wants counted."

@location:guard_headquarters
  name: "Guard Headquarters"
  description: "Stone walls and a duty roster nailed to the door."

# ── Player ───────────────────────────────────────────────────
@player:andrew
  name: "Andrew"
  location: market_district
  --[located_in]--> @location:market_district

@inventory/andrew/inventory:items
  --[contains]--> @inventory/andrew/item:shadow_blade

@inventory/andrew/item:shadow_blade
  name: "Shadow Blade"
  damage: 12
  rarity: rare

# ── NPCs ─────────────────────────────────────────────────────
# Grades: feeble | poor | average | capable | exceptional
# Proficient: any of athletics, acrobatics, stealth, perception, insight,
#             persuasion, deception, intimidation, sleight_of_hand, investigation

@npc:thomas_pellar
  name: "Thomas Pellar"
  occupation: merchant
  personality: cunning
  build: thin
  location: market_district
  physique: poor
  agility: average
  awareness: average
  presence: capable
  proficient: persuasion, insight, deception
  --[located_in]--> @location:market_district

@npc:jin_lyons
  name: "Jin Lyons"
  occupation: pickpocket
  personality: shifty
  build: thin
  location: market_district
  physique: poor
  agility: capable
  awareness: capable
  presence: poor
  proficient: perception, stealth, sleight_of_hand
  --[located_in]--> @location:market_district

@npc:john_smith
  name: "John Smith"
  occupation: guard
  personality: loyal
  build: large
  location: market_district
  physique: capable
  agility: average
  awareness: capable
  presence: average
  proficient: perception, athletics, intimidation
  --[reports_to]--> @npc:elias_roth
  --[located_in]--> @location:market_district

@npc:elias_roth
  name: "Elias Roth"
  occupation: guard_commander
  personality: stern
  build: average
  location: guard_headquarters
  physique: average
  agility: average
  awareness: average
  presence: exceptional
  proficient: insight, intimidation
  --[located_in]--> @location:guard_headquarters
