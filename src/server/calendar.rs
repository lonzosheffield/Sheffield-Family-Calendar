//! Google Calendar integration: a background task polls the configured
//! calendar with service account credentials and caches today's events.

use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::RwLock;

use crate::server::api::realtime;
use crate::shared::types::{CalendarEvent, ServerMessage};

pub const POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

static CACHE: OnceLock<RwLock<Vec<CalendarEvent>>> = OnceLock::new();

fn cache() -> &'static RwLock<Vec<CalendarEvent>> {
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

pub async fn cached_events() -> Vec<CalendarEvent> {
    cache().read().await.clone()
}

async fn store_events(events: Vec<CalendarEvent>) {
    let mut guard = cache().write().await;
    if *guard != events {
        *guard = events;
        drop(guard);
        // Protocol v2 (T1.2): the broadcast carries only the affected date;
        // clients refetch through `api::calendar::get_today_events`. Pushing
        // the payload itself is what made `CalendarUpdated` spoofable in v1
        // (G13), and it kept large frames on the broadcast channel.
        realtime::publish(&ServerMessage::CalendarUpdated {
            date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        });
    }
}

/// Credentials read from a Google service account JSON key file.
#[derive(Clone, Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
}

struct Config {
    account: ServiceAccount,
    calendar_id: String,
}

fn load_config() -> Option<Config> {
    let path = std::env::var("GOOGLE_SERVICE_ACCOUNT_JSON").ok()?;
    let calendar_id = std::env::var("GOOGLE_CALENDAR_ID").unwrap_or_else(|_| "primary".into());
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| tracing::warn!("cannot read {path}: {err}"))
        .ok()?;
    let account: ServiceAccount = serde_json::from_str(&raw)
        .map_err(|err| tracing::warn!("invalid service account json: {err}"))
        .ok()?;

    Some(Config {
        account,
        calendar_id,
    })
}

/// Spawn the 15 minute polling loop. Without credentials the hub still runs;
/// the calendar panel simply stays empty.
pub fn spawn_polling_task() {
    let Some(config) = load_config() else {
        tracing::info!("GOOGLE_SERVICE_ACCOUNT_JSON not set - skipping Google Calendar polling");
        return;
    };

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        loop {
            ticker.tick().await;
            match fetch_today(&client, &config).await {
                Ok(events) => {
                    store_events(events).await;
                    // T1.7 HANDOFF (applied by Boss at the wave 1-b close):
                    // `/health`'s `last_google_poll` is fed from here.
                    crate::server::health::record_google_poll_success(chrono::Local::now());
                }
                Err(err) => tracing::error!("google calendar poll failed: {err}"),
            }
        }
    });
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(serde::Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

async fn access_token(
    client: &reqwest::Client,
    account: &ServiceAccount,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        iss: account.client_email.clone(),
        scope: SCOPE.to_string(),
        aud: TOKEN_URL.to_string(),
        exp: now + 3600,
        iat: now,
    };

    let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes())?;
    let assertion = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &key,
    )?;

    let response: TokenResponse = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &assertion),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response.access_token)
}

#[derive(Deserialize)]
struct EventsResponse {
    #[serde(default)]
    items: Vec<GoogleEvent>,
}

#[derive(Deserialize)]
struct GoogleEvent {
    id: String,
    #[serde(default)]
    summary: Option<String>,
    start: GoogleTime,
    end: GoogleTime,
}

#[derive(Deserialize)]
struct GoogleTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
}

impl GoogleTime {
    fn value(&self) -> String {
        self.date_time
            .clone()
            .or_else(|| self.date.clone())
            .unwrap_or_default()
    }
}

async fn fetch_today(
    client: &reqwest::Client,
    config: &Config,
) -> Result<Vec<CalendarEvent>, Box<dyn std::error::Error + Send + Sync>> {
    let token = access_token(client, &config.account).await?;
    let start = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    let end = start + chrono::Duration::days(1);

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencode(&config.calendar_id)
    );

    let response: EventsResponse = client
        .get(url)
        .bearer_auth(token)
        .query(&[
            ("timeMin", rfc3339_local(start)),
            ("timeMax", rfc3339_local(end)),
            ("singleEvents", "true".to_string()),
            ("orderBy", "startTime".to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response
        .items
        .into_iter()
        .map(|event| CalendarEvent {
            id: event.id,
            summary: event.summary.unwrap_or_else(|| "(no title)".into()),
            all_day: event.start.date_time.is_none(),
            start: event.start.value(),
            end: event.end.value(),
        })
        .collect())
}

fn rfc3339_local(naive: chrono::NaiveDateTime) -> String {
    use chrono::TimeZone;
    chrono::Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| naive.and_utc().to_rfc3339())
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
