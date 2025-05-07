# Flare Validator API

High-performance API for querying Flare network validator data with strict eligibility filtering.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check for API status |
| `/api/validators` | GET | All validators with eligibility status |
| `/api/validators/eligible` | GET | Only eligible validators |
| `/api/validators/ineligible` | GET | Only ineligible validators |
| `/api/validators/top?limit=N` | GET | Top N validators by reward rate |
| `/api/validators/:id` | GET | Specific validator by ID |
| `/api/refresh` | POST | Force refresh of validator cache |

## Eligibility Criteria

A validator must meet ALL of the following conditions to be considered eligible:

1. **FTSO Anchor Feeds**: `ftso_scaling` must be `true`
2. **FTSO Block-Latency Feeds**: `ftso_fast_updates` must be `true`
3. **FDC**: `fdc` must be `true`
4. **Staking**: `staking` must be `true`
5. **Passes**: Must have exactly 3 passes
6. **Eligible for Reward**: Must be marked as eligible for rewards

## Build & Run

### Local Development

```bash
# Clone repository
git clone https://github.com/Rome-Blockchain-Labs/flare-validator-api.git
cd flare-validator-api

# Build and run
cargo run

# Build optimized release
cargo build --release
```

### Docker Deployment

```bash
# Build Docker image (minimized alpine-based)
docker build -t flare-validator-api .

# Run container
docker run -p 3000:3000 flare-validator-api
```

## Example Response

```json
{
  "timestamp": "2025-04-13T21:15:23.651Z",
  "count": 42,
  "validators": [
    {
      "id": 42,
      "name": "4DadsFTSO",
      "node_id": "NodeID-Ms5oKoFmzxNYgpAnppbY62GYmQcTQbXhV",
      "delegation_address": "0xC522E6A633545872f1afc0cdD7b2D96d97E3dE67",
      "conditions": {
        "ftso_anchor_feeds": true,
        "ftso_block_latency_feeds": true,
        "fdc": true,
        "staking": true,
        "passes": 3,
        "eligible_for_reward": true
      },
      "provider_stats": {
        "primary": 1559,
        "secondary": 9279,
        "availability": 100.0,
        "active": true
      },
      "reward_rates": {
        "wnat": 0.0006427240416880366,
        "mirror": 0.0003294899169163866,
        "pure": 0.0008760104359055636,
        "combined": 0.0018482243945099868
      }
    }
  ]
}
```
