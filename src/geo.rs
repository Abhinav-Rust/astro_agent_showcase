use reqwest::Client;
use serde::Deserialize;
use chrono::{NaiveDateTime, TimeZone, Offset};
use chrono_tz::Tz;
use tzf_rs::DefaultFinder;
use once_cell::sync::Lazy;

static FINDER: Lazy<DefaultFinder> = Lazy::new(|| DefaultFinder::new());

#[derive(Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
}

pub async fn get_location_data(city: &str, naive_dt: NaiveDateTime) -> Result<(f64, f64, f64), String> {
    let client = Client::builder()
        .user_agent("AstroAgent/1.0")
        .build()
        .map_err(|e| format!("Client Error: {}", e))?;

    let url = format!("https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1", city);
    let response = client.get(url).send().await
        .map_err(|e| format!("Failed to reach Nominatim: {}", e))?;

    let results: Vec<NominatimResult> = response.json().await
        .map_err(|e| format!("Failed to parse Nominatim JSON: {}", e))?;

    let result = results.get(0).ok_or_else(|| format!("City not found: {}", city))?;
    
    let lat: f64 = result.lat.parse().map_err(|_| "Invalid latitude from API")?;
    let lon: f64 = result.lon.parse().map_err(|_| "Invalid longitude from API")?;

    // tzf-rs takes (longitude, latitude)
    let tz_name = FINDER.get_tz_name(lon, lat);
    
    println!("Resolved Location: {} -> ({}, {})", city, lat, lon);
    println!("Resolved Timezone: {}", tz_name);

    let tz: Tz = tz_name.parse().map_err(|_| format!("Unsupported timezone: {}", tz_name))?;
    
    // Get historical UTC offset
    let dt = tz.from_local_datetime(&naive_dt)
        .earliest()
        .ok_or_else(|| "The provided local time is non-existent due to a DST transition.".to_string())?;

    let offset_seconds = dt.offset().fix().local_minus_utc();
    let offset_hours = offset_seconds as f64 / 3600.0;
    
    println!("Historical UTC Offset: {:.2} hours", offset_hours);

    Ok((lat, lon, offset_hours))
}
