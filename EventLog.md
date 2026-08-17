# Event Log

After recreating market.es, and running the program, stat blocks are being generated for all npcs, but edge targets with a '_' in
them are being skipped.

sent an action, classification and response took a long time

Found the issue, classification was dialogue by submiting "talk to the merchant"  this path hasn't been set to call LLM yet.

main issues I am struggling with right now is whether location should be an edge or a prop.