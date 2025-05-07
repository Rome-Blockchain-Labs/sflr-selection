use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use parking_lot::RwLock as PLRwLock;

const FLARE_API: &str = "https://flare-systems-explorer.flare.network/backend-url/api/v0";
const CACHE_TTL_SECS: u64 = 300; // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderStats {
    primary: Option<u32>,
    secondary: Option<u32>,
    availability: Option<f64>,
    active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Conditions {
    ftso_anchor_feeds: bool,
    ftso_block_latency_feeds: bool,
    fdc: bool,
    staking: bool,
    passes: u8,
    eligible_for_reward: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RewardRates {
    wnat: f64,
    mirror: f64,
    pure: f64,
    combined: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Validator {
    id: u32,
    name: String,
    node_id: Option<String>,
    delegation_address: Option<String>,
    conditions: Option<Conditions>,
    provider_stats: Option<ProviderStats>,
    reward_rates: Option<RewardRates>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorResponse {
    timestamp: String,
    total_validators: usize,
    eligible_count: usize,
    ineligible_count: usize,
    eligible_nodes: Vec<Validator>,
    ineligible_nodes: Vec<Validator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorsListResponse {
    timestamp: String,
    count: usize,
    validators: Vec<Validator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefreshResponse {
    success: bool,
    message: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsageResponse {
    api_name: String,
    version: String,
    endpoints: Vec<String>,
    timestamp: String,
}

// Raw data structures from Flare API
#[derive(Debug, Deserialize)]
struct FlareEntityMinConditions {
    ftso_scaling: Option<bool>,
    ftso_fast_updates: Option<bool>,
    fdc: Option<bool>,
    staking: Option<bool>,
    passes_held: Option<u8>,
    eligible_for_reward: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FlareRewards {
    reward_rate_wnat: Option<f64>,
    reward_rate_mirror: Option<f64>,
    reward_rate_pure: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FlareProviderSuccessRate {
    primary: Option<u32>,
    secondary: Option<u32>,
    availability: Option<u32>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FlareDenormalizedEntity {
    id: Option<u32>,
    node_ids: Vec<String>,
    public_key: Option<String>,
    submit_signatures_address: Option<String>,
    submit_address: Option<String>,
    signing_policy_address: Option<String>,
    delegation_address: Option<String>,
    rewards_signed: Option<u32>,
    uptime_signed: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct FlareSigningPolicy {
    delegation_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FlareEntity {
    id: u32,
    display_name: Option<String>,
    denormalizedentity: Option<FlareDenormalizedEntity>,
    entityminimalconditions: Option<FlareEntityMinConditions>,
    rewards: Option<FlareRewards>,
    providersuccessrate: Option<FlareProviderSuccessRate>,
    denormalizedsigningpolicy: Option<FlareSigningPolicy>,
}

#[derive(Debug, Deserialize)]
struct FlareEntityList {
    results: Vec<FlareEntity>,
}

struct AppState {
    http_client: Client,
    cache: PLRwLock<Option<(ValidatorResponse, SystemTime)>>,
}

async fn fetch_validator_data(state: &AppState) -> Result<ValidatorResponse, reqwest::Error> {
    // First check cache
    {
        let cache_read = state.cache.read();
        if let Some((data, timestamp)) = &*cache_read {
            let elapsed = SystemTime::now().duration_since(*timestamp).unwrap_or(Duration::from_secs(CACHE_TTL_SECS + 1));
            if elapsed < Duration::from_secs(CACHE_TTL_SECS) {
                return Ok(data.clone());
            }
        }
    }

    // Cache miss or expired, fetch fresh data
    let url = format!("{}/entity?limit=200&offset=0", FLARE_API);
    let response = state.http_client.get(&url).send().await?;
    let entity_list: FlareEntityList = response.json().await?;

    let mut eligible_nodes = Vec::new();
    let mut ineligible_nodes = Vec::new();

    for entity in &entity_list.results {
        let validator = process_entity(entity);

        // Check eligibility based on our strict criteria
        if let Some(cond) = &validator.conditions {
            if cond.eligible_for_reward &&
               cond.ftso_anchor_feeds &&
               cond.ftso_block_latency_feeds &&
               cond.fdc &&
               cond.staking &&
               cond.passes == 3 {
                eligible_nodes.push(validator);
            } else {
                ineligible_nodes.push(validator);
            }
        } else {
            ineligible_nodes.push(validator);
        }
    }

    // Sort eligible nodes by combined reward rate
    eligible_nodes.sort_by(|a, b| {
        let rate_a = a.reward_rates.as_ref().map_or(0.0, |r| r.combined);
        let rate_b = b.reward_rates.as_ref().map_or(0.0, |r| r.combined);
        rate_b.partial_cmp(&rate_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    let response = ValidatorResponse {
        timestamp: chrono::Utc::now().to_rfc3339(),
        total_validators: entity_list.results.len(),
        eligible_count: eligible_nodes.len(),
        ineligible_count: ineligible_nodes.len(),
        eligible_nodes,
        ineligible_nodes,
    };

    // Update cache
    {
        let mut cache_write = state.cache.write();
        *cache_write = Some((response.clone(), SystemTime::now()));
    }

    Ok(response)
}

fn process_entity(entity: &FlareEntity) -> Validator {
    // Extract conditions
    let conditions = entity.entityminimalconditions.as_ref().map(|c| Conditions {
        ftso_anchor_feeds: c.ftso_scaling.unwrap_or(false),
        ftso_block_latency_feeds: c.ftso_fast_updates.unwrap_or(false),
        fdc: c.fdc.unwrap_or(false),
        staking: c.staking.unwrap_or(false),
        passes: c.passes_held.unwrap_or(0),
        eligible_for_reward: c.eligible_for_reward.unwrap_or(false),
    });

    // Extract reward rates
    let reward_rates = entity.rewards.as_ref().map(|r| {
        let wnat = r.reward_rate_wnat.unwrap_or(0.0);
        let mirror = r.reward_rate_mirror.unwrap_or(0.0);
        let pure = r.reward_rate_pure.unwrap_or(0.0);

        RewardRates {
            wnat,
            mirror,
            pure,
            combined: wnat + mirror + pure,
        }
    });

    // Extract provider stats
    let provider_stats = entity.providersuccessrate.as_ref().map(|p| ProviderStats {
        primary: p.primary,
        secondary: p.secondary,
        availability: p.availability.map(|a| a as f64 / 100.0),
        active: p.active,
    });

    Validator {
        id: entity.id,
        name: entity.display_name.clone().unwrap_or_else(|| "Unknown".to_string()),
        node_id: entity.denormalizedentity.as_ref().and_then(|d| d.node_ids.first().cloned()),
        delegation_address: entity.denormalizedsigningpolicy.as_ref().and_then(|d| d.delegation_address.clone()),
        conditions,
        provider_stats,
        reward_rates,
    }
}

#[get("/")]
async fn usage() -> impl Responder {
    HttpResponse::Ok().json(UsageResponse {
        api_name: "Flare Validator API".to_string(),
        version: "1.0.0".to_string(),
        endpoints: vec![
            "/health".to_string(),
            "/api/validators".to_string(),
            "/api/validators/eligible".to_string(),
            "/api/validators/ineligible".to_string(),
            "/api/validators/top?limit=N".to_string(),
            "/api/validators/{id}".to_string(),
            "/api/refresh".to_string(),
        ],
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

#[get("/health")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

#[get("/api/validators")]
async fn get_all_validators(state: web::Data<Arc<AppState>>) -> impl Responder {
    match fetch_validator_data(&state).await {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to fetch validator data"
        })),
    }
}

#[get("/api/validators/eligible")]
async fn get_eligible_validators(state: web::Data<Arc<AppState>>) -> impl Responder {
    match fetch_validator_data(&state).await {
        Ok(data) => HttpResponse::Ok().json(ValidatorsListResponse {
            timestamp: data.timestamp,
            count: data.eligible_count,
            validators: data.eligible_nodes,
        }),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to fetch eligible validators"
        })),
    }
}

#[get("/api/validators/ineligible")]
async fn get_ineligible_validators(state: web::Data<Arc<AppState>>) -> impl Responder {
    match fetch_validator_data(&state).await {
        Ok(data) => HttpResponse::Ok().json(ValidatorsListResponse {
            timestamp: data.timestamp,
            count: data.ineligible_count,
            validators: data.ineligible_nodes,
        }),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to fetch ineligible validators"
        })),
    }
}

#[get("/api/validators/top")]
async fn get_top_validators(
    state: web::Data<Arc<AppState>>,
    query: web::Query<std::collections::HashMap<String, String>>
) -> impl Responder {
    // top 50
    let limit = query.get("limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(50);

    match fetch_validator_data(&state).await {
        Ok(data) => {
            let count = std::cmp::min(limit, data.eligible_nodes.len());
            HttpResponse::Ok().json(ValidatorsListResponse {
                timestamp: data.timestamp,
                count,
                validators: data.eligible_nodes.into_iter().take(limit).collect(),
            })
        },
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to fetch top validators"
        })),
    }
}

#[get("/api/validators/{id}")]
async fn get_validator_by_id(
    state: web::Data<Arc<AppState>>,
    path: web::Path<u32>,
) -> impl Responder {
    let validator_id = path.into_inner();

    match fetch_validator_data(&state).await {
        Ok(data) => {
            let validator = data.eligible_nodes.iter()
                .chain(data.ineligible_nodes.iter())
                .find(|v| v.id == validator_id);

            match validator {
                Some(v) => HttpResponse::Ok().json(v),
                None => HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Validator not found"
                })),
            }
        },
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to fetch validator details"
        })),
    }
}

#[post("/api/refresh")]
async fn force_refresh(state: web::Data<Arc<AppState>>) -> impl Responder {
    // Clear the cache
    {
        let mut cache_write = state.cache.write();
        *cache_write = None;
    }

    // Fetch fresh data
    match fetch_validator_data(&state).await {
        Ok(data) => HttpResponse::Ok().json(RefreshResponse {
            success: true,
            message: "Cache refreshed successfully".to_string(),
            timestamp: data.timestamp,
        }),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to refresh cache"
        })),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let http_client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client");

    let state = Arc::new(AppState {
        http_client,
        cache: PLRwLock::new(None),
    });

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    log::info!("Starting server at {}", addr);

    // Print usage on startup
    println!("Flare Validator API");
    println!("Usage:");
    println!("  /                        - API usage information");
    println!("  /health                  - Health check endpoint");
    println!("  /api/validators          - List all validators");
    println!("  /api/validators/eligible - List eligible validators");
    println!("  /api/validators/ineligible - List ineligible validators");
    println!("  /api/validators/top      - List top validators (default: 50)");
    println!("  /api/validators/top?limit=N - List top N validators");
    println!("  /api/validators/{{id}}     - Get validator by ID");
    println!("  /api/refresh             - Force refresh cache (POST)");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&state)))
            .service(usage)
            .service(health_check)
            .service(get_all_validators)
            .service(get_eligible_validators)
            .service(get_ineligible_validators)
            .service(get_top_validators)
            .service(get_validator_by_id)
            .service(force_refresh)
    })
    .workers(num_cpus::get())
    .bind(addr)?
    .run()
    .await
}
