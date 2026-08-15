@location:market_district
@location:docks
@location:guard_headquarters  

@player:andrew
  --[located_in]--> market_district

@inventory/andrew/item:shadow_blade
  name: "Shadow Blade"
  damage: 12
  rarity: rare
  --[owned_by]--> @player:andrew

@npc:thomas_pellar
  occupation: merchant
  personality: cunning
  build: thin
  --[located_in]--> market_district

@npc:jin_lyons
  occupation: pickpocket
  personality: shifty
  build: thin
  --[located_in]--> market_district

@npc:john_smith
  occupation: guard
  personality: loyal
  build: large
  --[reports_to]--> guard_commander
  --[located_in]--> market_district

@npc:elias_roth
  occupation: guard_commander
  personality: stern
  build: average
  --[located_in]--> guard_headquarters