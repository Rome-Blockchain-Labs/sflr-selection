curl -s 0.0.0.0:3000/api/validators/ineligible | jq -r '
.validators | map(select(.delegation_address != null or .node_id != null)) | map([
  .id,
  .name,
  .node_id,
  .delegation_address,
  (.conditions.ftso_anchor_feeds // false),
  (.conditions.ftso_block_latency_feeds // false),
  (.conditions.fdc // false),
  (.conditions.staking // false),
  (.conditions.passes // 0),
  (.conditions.eligible_for_reward // false),
  (.provider_stats.primary // ""),
  (.provider_stats.secondary // ""),
  (.provider_stats.availability // ""),
  (.provider_stats.active // false),
  (.reward_rates.wnat // 0),
  (.reward_rates.mirror // 0),
  (.reward_rates.pure // 0),
  (.reward_rates.combined // 0)
]) | 
  ["ID", "Name", "NodeID", "DelegationAddress", "FTSO_Anchor", "FTSO_Block_Latency", "FDC", "Staking", "Passes", "EligibleForReward", "Primary", "Secondary", "Availability", "Active", "RewardRate_WNAT", "RewardRate_Mirror", "RewardRate_Pure", "RewardRate_Combined"] as $headers |
  [$headers] + . | 
  map(@csv) | 
  join("\n")' > banlist.csv
