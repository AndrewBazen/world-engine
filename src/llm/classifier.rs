use super::call_ollama;

pub enum InputCategory {
    Action,
    Query,
    Dialogue,
    Movement,
}

pub async fn classify_input(input: &str, location: &str) -> Result<InputCategory, String> {
    let prompt = format!(
        r#"Classify this player input into exactly one category.

Categories:
- action: player does something that changes the world (attack, steal, cast, use, drop, pick up, buy, sell, give)
- query: player wants information (look, examine, check inventory, what do I see, where am I)
- dialogue: player wants to talk to someone (talk to, ask, tell, greet, persuade, threaten)
- movement: player wants to go somewhere (go to, walk to, enter, leave, travel, move to)

Player location: {location}
Player input: "{input}"

Respond with ONLY the category name. Nothing else."#,
        location = location,
        input = input,
    );

    let response = call_ollama(super::CLASSIFIER_MODEL, &prompt).await?;
    let category = response.trim().to_lowercase();

    match category.as_str() {
        "action" => Ok(InputCategory::Action),
        "query" => Ok(InputCategory::Query),
        "dialogue" => Ok(InputCategory::Dialogue),
        "movement" => Ok(InputCategory::Movement),
        _ => {
            eprintln!("classifier returned unexpected category: {}", category);
            Ok(InputCategory::Action) // default to action
        }
    }
}

pub async fn resolve_location(input: &str, locations: &[String]) -> Result<String, String> {
    let location_list = locations.join(", ");

    let prompt = format!(
        r#"Given these locations: {locations}

The player said: "{input}"

Respond with ONLY the location ID that best matches. Nothing else."#,
        locations = location_list,
        input = input,
    );

    let response = call_ollama(super::CLASSIFIER_MODEL, &prompt).await?;
    Ok(response.trim().to_string())
}

pub async fn should_npc_act(
    npc_name: &str,
    occupation: &str,
    personality: &str,
    relationships: &str,
    signal_context: &str,
    signal_strength: f64,
) -> Result<bool, String> {
    let is_direct = signal_context.to_lowercase().contains(&npc_name.to_lowercase());

    let involvement = if is_direct {
        "This directly involves you."
    } else {
        "This happened nearby."
    };
    let prompt = format!(
        r#"You are {name}, a {occupation}. Your personality: {personality}.
Your relationships: {relationships}.

You just noticed (strength {strength:.1}/1.0): "{signal}"

{involvement}

Would you notice and react to this, or would you carry on as if nothing happened?
Respond with ONLY "act" or "ignore". Nothing else."#,
        name = npc_name,
        occupation = occupation,
        personality = personality,
        relationships = relationships,
        strength = signal_strength,
        signal = signal_context,
    );

    let response = call_ollama(super::CLASSIFIER_MODEL, &prompt).await?;
    let answer = response.trim().to_lowercase();

    Ok(answer.contains("act"))
}