use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep, Duration};
use tracing::{debug, warn};

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    country: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    #[serde(rename = "as")]
    asn: Option<String>,
    status: String,
}

#[derive(Debug)]
pub struct GeoLookup {
    client: reqwest::Client,
    cache: Arc<Mutex<HashMap<IpAddr, (String, String)>>>, // IP -> (country, country_code)
    rate_limiter: Arc<Semaphore>, // Global rate limiter
}

impl GeoLookup {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            cache: Arc::new(Mutex::new(HashMap::new())),
            rate_limiter: Arc::new(Semaphore::new(3)), // Allow up to 3 concurrent requests
        }
    }

    pub async fn lookup_country(&self, ip: IpAddr) -> Result<(String, String)> {
        // Check cache first
        {
            let cache = self.cache.lock().await;
            if let Some(result) = cache.get(&ip) {
                debug!("Cache hit for IP {}: {}", ip, result.0);
                return Ok(result.clone());
            }
        }

        // Acquire rate limiter permit - allow up to 3 concurrent requests
        let _permit = self.rate_limiter.acquire().await.unwrap();
        
        // Rate limiting - ip-api.com allows 45 requests per minute for free
        // We'll do 1 request per 1.5 seconds (40/minute) with up to 3 concurrent
        sleep(Duration::from_millis(1500)).await;

        let url = format!("http://ip-api.com/json/{}", ip);
        
        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<IpApiResponse>().await {
                        Ok(data) => {
                            if data.status == "success" {
                                let country = data.country.unwrap_or_else(|| "Unknown".to_string());
                                let country_code = data.country_code.unwrap_or_else(|| "UN".to_string());
                                
                                // Cache the result
                                {
                                    let mut cache = self.cache.lock().await;
                                    cache.insert(ip, (country.clone(), country_code.clone()));
                                }
                                
                                debug!("Looked up IP {}: {} ({})", ip, country, country_code);
                                Ok((country, country_code))
                            } else {
                                warn!("Failed to lookup IP {}: {}", ip, data.status);
                                Ok(("Unknown".to_string(), "UN".to_string()))
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse response for IP {}: {}", ip, e);
                            Ok(("Unknown".to_string(), "UN".to_string()))
                        }
                    }
                } else {
                    warn!("HTTP error for IP {}: {}", ip, response.status());
                    Ok(("Unknown".to_string(), "UN".to_string()))
                }
            }
            Err(e) => {
                warn!("Network error for IP {}: {}", ip, e);
                Ok(("Unknown".to_string(), "UN".to_string()))
            }
        }
    }

    pub async fn lookup_countries_batch(&self, ips: Vec<IpAddr>) -> HashMap<IpAddr, (String, String)> {
        let mut results = HashMap::new();
        
        for ip in ips {
            match self.lookup_country(ip).await {
                Ok(result) => {
                    results.insert(ip, result);
                }
                Err(e) => {
                    warn!("Failed to lookup country for {}: {}", ip, e);
                    results.insert(ip, ("Unknown".to_string(), "UN".to_string()));
                }
            }
        }
        
        results
    }
}