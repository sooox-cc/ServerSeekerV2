use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use regex::Regex;
use std::sync::OnceLock;
use sqlx::{PgPool, Row};
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
struct Server {
    address: String,
    port: i32,
    software: Option<String>,
    version: Option<String>,
    protocol: Option<i32>,
    description_formatted: Option<String>,
    online_players: Option<i32>,
    max_players: Option<i32>,
    country: Option<String>,
    first_seen: i32,
    last_seen: i32,
    status: String,
    visited_at: Option<chrono::DateTime<chrono::Utc>>,
    notes: Option<String>,
    rating: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ServerFilters {
    status: Option<String>,
    software: Option<String>,
    country: Option<String>,
    min_players: Option<i32>,
    max_players: Option<i32>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct VisitUpdate {
    status: Option<String>,
    notes: Option<String>,
    rating: Option<i32>,
}

#[derive(Debug, Serialize)]
struct ScanStats {
    total_servers: i64,
    visited_servers: i64,
    skipped_servers: i64,
    whitelisted_servers: i64,
    unvisited_servers: i64,
    unique_software_types: Vec<String>,
    unique_countries: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    db: PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Database connection
    let database_url = "postgresql://postgres:mysecretpassword@localhost:5433/postgres";
    let pool = sqlx::PgPool::connect(database_url).await?;

    let state = AppState { db: pool };

    // Build the router
    let app = Router::new()
        .route("/api/servers", get(list_servers))
        .route("/api/servers/:address/:port/visit", post(mark_visited))
        .route("/api/servers/:address/:port/visit", put(update_visit))
        .route("/api/stats", get(get_stats))
        .nest_service("/", ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn minecraft_color_codes_to_html(text: &str) -> String {
    static COLOR_REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = COLOR_REGEX.get_or_init(|| {
        Regex::new(r"§([0-9a-fklmnor])").unwrap()
    });

    let mut result = String::new();
    let mut last_end = 0;
    let mut open_span = false;

    for caps in regex.captures_iter(text) {
        let full_match = caps.get(0).unwrap();
        let code = caps.get(1).unwrap().as_str();
        
        // Add text before this match
        result.push_str(&text[last_end..full_match.start()]);
        
        // Close previous span if open
        if open_span {
            result.push_str("</span>");
            open_span = false;
        }
        
        // Add new span based on color code
        match code {
            "0" => { result.push_str(r#"<span class="mc-black">"#); open_span = true; },
            "1" => { result.push_str(r#"<span class="mc-dark-blue">"#); open_span = true; },
            "2" => { result.push_str(r#"<span class="mc-dark-green">"#); open_span = true; },
            "3" => { result.push_str(r#"<span class="mc-dark-aqua">"#); open_span = true; },
            "4" => { result.push_str(r#"<span class="mc-dark-red">"#); open_span = true; },
            "5" => { result.push_str(r#"<span class="mc-dark-purple">"#); open_span = true; },
            "6" => { result.push_str(r#"<span class="mc-gold">"#); open_span = true; },
            "7" => { result.push_str(r#"<span class="mc-gray">"#); open_span = true; },
            "8" => { result.push_str(r#"<span class="mc-dark-gray">"#); open_span = true; },
            "9" => { result.push_str(r#"<span class="mc-blue">"#); open_span = true; },
            "a" => { result.push_str(r#"<span class="mc-green">"#); open_span = true; },
            "b" => { result.push_str(r#"<span class="mc-aqua">"#); open_span = true; },
            "c" => { result.push_str(r#"<span class="mc-red">"#); open_span = true; },
            "d" => { result.push_str(r#"<span class="mc-light-purple">"#); open_span = true; },
            "e" => { result.push_str(r#"<span class="mc-yellow">"#); open_span = true; },
            "f" => { result.push_str(r#"<span class="mc-white">"#); open_span = true; },
            "k" => { result.push_str(r#"<span class="mc-obfuscated">"#); open_span = true; },
            "l" => { result.push_str(r#"<span class="mc-bold">"#); open_span = true; },
            "m" => { result.push_str(r#"<span class="mc-strikethrough">"#); open_span = true; },
            "n" => { result.push_str(r#"<span class="mc-underline">"#); open_span = true; },
            "o" => { result.push_str(r#"<span class="mc-italic">"#); open_span = true; },
            "r" => { 
                if open_span {
                    result.push_str("</span>");
                    open_span = false;
                }
            },
            _ => {}
        }
        
        last_end = full_match.end();
    }

    // Add remaining text
    result.push_str(&text[last_end..]);
    
    // Close any remaining open span
    if open_span {
        result.push_str("</span>");
    }

    result
}

async fn list_servers(
    State(state): State<AppState>,
    Query(filters): Query<ServerFilters>,
) -> Result<Json<Vec<Server>>, StatusCode> {
    let limit = filters.limit.unwrap_or(100).min(1000);
    let offset = filters.offset.unwrap_or(0);

    let mut query = r#"
        SELECT 
            host(s.address) as address,
            s.port,
            s.software::text,
            s.version,
            s.protocol,
            s.description_formatted,
            s.online_players,
            s.max_players,
            s.country,
            s.first_seen,
            s.last_seen,
            COALESCE(v.status::text, 'not_visited') as status,
            v.visited_at,
            v.notes,
            v.rating
        FROM servers s
        LEFT JOIN server_visits v ON s.address = v.address AND s.port = v.port
        WHERE 1=1
    "#.to_string();

    if let Some(status) = &filters.status {
        match status.as_str() {
            "visited" => query.push_str(" AND v.status = 'visited'"),
            "skipped" => query.push_str(" AND v.status = 'skipped'"),
            "whitelisted" => query.push_str(" AND v.status = 'whitelisted'"),
            "not_visited" => query.push_str(" AND v.address IS NULL"),
            _ => {}
        }
    }

    if let Some(software) = &filters.software {
        if software.contains(',') {
            let software_list: Vec<&str> = software.split(',').collect();
            let quoted_list = software_list.iter()
                .map(|s| format!("'{}'", s.trim()))
                .collect::<Vec<_>>()
                .join(",");
            query.push_str(&format!(" AND s.software::text IN ({})", quoted_list));
        } else {
            query.push_str(&format!(" AND s.software::text = '{}'", software));
        }
    }

    if let Some(country) = &filters.country {
        query.push_str(&format!(" AND s.country = '{}'", country));
    }

    if let Some(min_players) = filters.min_players {
        query.push_str(&format!(" AND s.online_players >= {}", min_players));
    }

    if let Some(max_players) = filters.max_players {
        query.push_str(&format!(" AND s.max_players <= {}", max_players));
    }

    // Handle sorting
    let sort_field = filters.sort_by.as_deref().unwrap_or("last_seen");
    let sort_order = filters.sort_order.as_deref().unwrap_or("desc").to_uppercase();
    
    let valid_sort_fields = vec![
        "last_seen", "first_seen", "online_players", "max_players", 
        "address", "software", "country", "rating"
    ];
    
    if valid_sort_fields.contains(&sort_field) && (sort_order == "ASC" || sort_order == "DESC") {
        match sort_field {
            "address" => query.push_str(&format!(" ORDER BY host(s.address) {}", sort_order)),
            "software" => query.push_str(&format!(" ORDER BY s.software::text {}", sort_order)),
            "rating" => query.push_str(&format!(" ORDER BY v.rating {} NULLS LAST", sort_order)),
            _ => query.push_str(&format!(" ORDER BY s.{} {}", sort_field, sort_order)),
        }
    } else {
        query.push_str(" ORDER BY s.last_seen DESC");
    }
    
    query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let rows = sqlx::query(&query)
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let servers: Vec<Server> = rows
        .into_iter()
        .map(|row| {
            let description: Option<String> = row.get("description_formatted");
            let processed_description = description.map(|desc| minecraft_color_codes_to_html(&desc));
            
            Server {
                address: row.get("address"),
                port: row.get("port"),
                software: row.get("software"),
                version: row.get("version"),
                protocol: row.get("protocol"),
                description_formatted: processed_description,
                online_players: row.get("online_players"),
                max_players: row.get("max_players"),
                country: row.get("country"),
                first_seen: row.get("first_seen"),
                last_seen: row.get("last_seen"),
                status: row.get("status"),
                visited_at: row.get("visited_at"),
                notes: row.get("notes"),
                rating: row.get("rating"),
            }
        })
        .collect();

    Ok(Json(servers))
}

async fn mark_visited(
    State(state): State<AppState>,
    Path((address, port)): Path<(String, String)>,
    Json(visit): Json<VisitUpdate>,
) -> Result<StatusCode, StatusCode> {
    let port: i32 = port.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let address: std::net::IpAddr = address.parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    let status = visit.status.as_deref().unwrap_or("visited");
    
    sqlx::query(
        "INSERT INTO server_visits (address, port, status, notes, rating) VALUES ($1, $2, $3::visit_status, $4, $5)
         ON CONFLICT (address, port) DO UPDATE SET 
         status = EXCLUDED.status, visited_at = CURRENT_TIMESTAMP, notes = EXCLUDED.notes, rating = EXCLUDED.rating"
    )
    .bind(address)
    .bind(port)
    .bind(status)
    .bind(&visit.notes)
    .bind(visit.rating)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn update_visit(
    State(state): State<AppState>,
    Path((address, port)): Path<(String, String)>,
    Json(visit): Json<VisitUpdate>,
) -> Result<StatusCode, StatusCode> {
    let port: i32 = port.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let address: std::net::IpAddr = address.parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut query = "UPDATE server_visits SET notes = $3, rating = $4".to_string();
    let bind_index = 5;
    
    if let Some(_status) = &visit.status {
        query.push_str(&format!(", status = ${}::visit_status", bind_index));
    }
    
    query.push_str(" WHERE address = $1 AND port = $2");
    
    let mut db_query = sqlx::query(&query)
        .bind(address)
        .bind(port)
        .bind(&visit.notes)
        .bind(visit.rating);
    
    if let Some(status) = &visit.status {
        db_query = db_query.bind(status);
    }
    
    db_query
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn get_stats(State(state): State<AppState>) -> Result<Json<ScanStats>, StatusCode> {
    let total_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers")
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let visited_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM server_visits WHERE status = 'visited'")
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let skipped_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM server_visits WHERE status = 'skipped'")
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let whitelisted_servers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM server_visits WHERE status = 'whitelisted'")
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let software_types: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT software::text FROM servers WHERE software IS NOT NULL ORDER BY software::text"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let countries: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT country FROM servers WHERE country IS NOT NULL AND country != 'Unknown' ORDER BY country"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stats = ScanStats {
        total_servers: total_servers.0,
        visited_servers: visited_servers.0,
        skipped_servers: skipped_servers.0,
        whitelisted_servers: whitelisted_servers.0,
        unvisited_servers: total_servers.0 - visited_servers.0 - skipped_servers.0 - whitelisted_servers.0,
        unique_software_types: software_types.into_iter().map(|s| s.0).collect(),
        unique_countries: countries.into_iter().map(|c| c.0).collect(),
    };

    Ok(Json(stats))
}

