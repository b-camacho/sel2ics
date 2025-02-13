use chrono::{DateTime, Utc, TimeZone, NaiveDateTime};
use chrono_tz::Tz;
use http_body_util::BodyExt;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto::Builder;
use regex::Regex;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use anyhow::Result;
use anyhow::Context;

#[derive(Deserialize, Debug)]
struct CalendarRequest {
    text: String
}


#[derive(Serialize)]
struct LlmMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct LlmRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<LlmMessage>,
}

#[derive(Deserialize)]
struct LlmResponse {
    content: Vec<Content>,
}

// unwrapped means we stripped the <thinking> and <answer> tags
// this should just be the json text
#[derive(Serialize, Debug)]
struct LlmResponseUnwrapped {
    ical_as_json: String,
}

#[derive(Deserialize)]
struct Content {
    text: String,
}

// exmample ical json {"name": "Repotting class", "start_time": "2022-09-27 18:00:00", "end_time": "2022-09-27 20:00:00", "start_time_zone": "EDT", "end_time_zone": "EDT", "location": "6BC Botanical Garden, New York"}
#[derive(Deserialize, Debug)]
struct IcalEventJson {
    name: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    start_time_zone: Option<String>,
    end_time_zone: Option<String>,
    location: Option<String>,
}

impl IcalEventJson {
    /// parse from json. start_time is required. name and end time are inferred if missing. end_time, location and timezones left blank if missing
    fn parse(&self) -> anyhow::Result<IcalEvent> {
        use std::str::FromStr;
        
        let name = self.name.clone().unwrap_or("Event".to_string());

        let start_time: NaiveDateTime = self.start_time
            .as_ref()
            .map(|st| NaiveDateTime::parse_from_str(st, "%Y-%m-%d %H:%M:%S"))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse start time: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("start_time is required"))?;

        let start_time_zone = self.start_time_zone
            .as_ref()
            .map(|tz| chrono_tz::Tz::from_str(tz))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse start timezone: {}", e))?;

        let end_time = self.end_time
            .as_ref()
            .map(|et| NaiveDateTime::parse_from_str(et, "%Y-%m-%d %H:%M:%S"))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse end time: {}", e))?
            .unwrap_or(start_time + chrono::Duration::hours(1));

        let end_time_zone = self.end_time_zone
            .as_ref()
            .map(|tz| chrono_tz::Tz::from_str(tz))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse end timezone: {}", e))?;

        let location = self.location.clone();

        Ok(IcalEvent {
            name,
            start_time,
            end_time,
            start_time_zone,
            end_time_zone,
            location,
        })
    }
}

#[derive(Debug)]
struct IcalEvent {
    name: String,
    start_time: NaiveDateTime,
    end_time: NaiveDateTime,
    start_time_zone: Option<chrono_tz::Tz>,
    end_time_zone: Option<chrono_tz::Tz>,
    location: Option<String>,
}


impl IcalEvent {
    fn to_ics(&self) -> String {
        let mut output = String::new();
        output.push_str("BEGIN:VEVENT\r\n");
        output.push_str(&format!("SUMMARY:{}\r\n", self.name));
        output.push_str(&format!("DTSTART:{}\r\n", to_ical_ts(&self.start_time, &self.start_time_zone)));
        output.push_str(&format!("DTEND:{}\r\n", to_ical_ts(&self.end_time, &self.end_time_zone)));

        if let Some(location) = self.location.clone() {
            output.push_str(&format!("LOCATION:{}\r\n", location));
        }

        // silly protocol stuff
        output.push_str(&format!("UID:{}\r\n", Uuid::new_v4()));
        output.push_str(&format!("DTSTAMP:{}\r\n", to_ical_ts(&Utc::now().naive_utc(), &None)));
        output.push_str("PRODID:-//UnemployedLucia//sel2ics//EN\r\n");

        output.push_str("END:VEVENT\r\n");
        output
    }
}

fn to_ical_ts(t: &chrono::NaiveDateTime, z: &Option<chrono_tz::Tz>) -> String {
    // if we know the tz, specify it explicitly as in DTSTART;TZID=America/New_York:20250212T150000
    // if we do not know the tz, use the "floating" format as in DTSTART:20250212T150000
    // the latter makes the calendar app use ther users's local timezone
    let tzid = z.map(|z| format!("TZID={}:", z.name()));
    format!("{}{}", tzid.unwrap_or("".to_owned()), t.format("%Y%m%dT%H%M%S").to_string())
}

fn build_llm_request(text: &str) -> LlmRequest {
    let system_prompt = r#"You will be given natural language text describing an event. The event may have a start time, end time, duration, location or name. Produce a json describing the event. Use this as an example:
<example>
{"name": "Repotting class", "start_time": "2022-09-27 18:00:00", "end_time": "2022-09-27 20:00:00", "start_time_zone": "America/New_York", "end_time_zone": "America/New_York", "location": "6BC Botanical Garden, New York"}
</example>
If you need to think, use <thinking></thinking> tags. Your answer should be a single json object. Put your answer in <answer></answer> tags.
"#;

    LlmRequest {
        model: "claude-3-haiku-20240307".to_string(),
        max_tokens: 1000,
        system: system_prompt.to_string(),
        messages: vec![LlmMessage {
            role: "user".to_string(),
            content: text.to_string(),
        }],
    }
}

// example claude response:
//<thinking>
//This text describes an event with the following details:
//Name: Cosmo Sheldrake - Amex Presale Tickets™
//Date: Thursday, July 10, 2025
//Time: 8:00 PM
//Location: Racket, New York, NY
//It doesn't specify an end time or time zone, so I'll leave those out of the JSON.
//</thinking>
//<answer>
//{
//  "name": "Cosmo Sheldrake - Amex Presale Tickets™",
//  "start_time": "2025-07-10 20:00:00",
//  "start_time_zone": "America/New_York",
//  "location": "Racket, New York, NY"
//}
//</answer>

async fn query_llm(ar: LlmRequest) -> anyhow::Result<LlmResponse> {
    let api_key = env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY must be set")?;
    let api_url = "https://api.anthropic.com/v1/messages";

    let client = reqwest::Client::new();
    let response = client
        .post(api_url)
        .header("Content-Type", "application/json")
        .header("X-API-Key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("llm-version", "2023-06-01")
        .json(&ar)
        .send()
        .await;

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                Ok(resp.json::<LlmResponse>().await?)
            } else {
                let error_message = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                Err(anyhow::anyhow!("API error: {}", error_message))
            }
        }
        Err(e) => Err(anyhow::anyhow!("Request failed: {}", e)),
    }
}

fn unwrap_llm_response(ar: LlmResponse) -> anyhow::Result<LlmResponseUnwrapped> {
    let assistant_message = &ar.content[0].text;

    if let Some(ical_as_json) = extract_ical_as_json(assistant_message) {
        let calendar_response = LlmResponseUnwrapped { ical_as_json };
        Ok(calendar_response)
    } else {
        Err(anyhow::anyhow!("Failed to extract iCal content"))
    }
}

async fn handle_ical(cr: CalendarRequest) -> anyhow::Result<String> {
    log::info!("parsed: {cr:?}");
    let llm_req = build_llm_request(&cr.text);
    let llm_res = query_llm(llm_req).await?;
    let user_res = unwrap_llm_response(llm_res)?;
    log::info!("user_res: {user_res:?}");
    let ical_event_json: IcalEventJson = serde_json::from_str(&user_res.ical_as_json)
        .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;
    let ical_event = ical_event_json.parse()?;
    Ok(ical_event.to_ics())
}

async fn route(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<String>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/cal") => {
            let body_bytes = req.collect().await.unwrap().to_bytes();
            let calendar_request: CalendarRequest = serde_json::from_slice(&body_bytes).unwrap();
            match handle_ical(calendar_request).await {
                Ok(ical_event_str) =>  Ok(Response::builder()
                .status(StatusCode::OK)
                .body(ical_event_str)
                .unwrap()),
                Err(e) => {
                    log::error!("Calendar request failed: {}", e);
                    Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(format!("Failed to process calendar request: {}", e))
                    .unwrap())
                }
            }}
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not Found".to_string())
            .unwrap()),
    }
}

fn extract_ical_as_json(text: &str) -> Option<String> {
    let re = Regex::new(r"<answer>([\s\S]*?)</answer>").unwrap();
    re.captures(text)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string()).parse::<u16>().unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = TcpListener::bind(addr).await?;
    log::info!("We up on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = Builder::new(hyper_util::rt::TokioExecutor::new())
                .serve_connection(io, service_fn(route))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn ical_event_converts_to_valid_ical_string() {
        // Test minimal event (just start time)
        let minimal_event = IcalEvent {
            name: "Event".to_string(),
            start_time: NaiveDateTime::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(), chrono::NaiveTime::from_hms_opt(14, 30, 0).unwrap()),
            end_time: NaiveDateTime::new(NaiveDate::from_ymd_opt(2024, 3, 15).unwrap(), chrono::NaiveTime::from_hms_opt(15, 30, 0).unwrap()),
            start_time_zone: None,
            end_time_zone: None,
            location: None,
        };

        dbg!(&minimal_event);

        let result = minimal_event.to_ics();
        dbg!(&result);
        assert!(result.contains("BEGIN:VEVENT"));
        assert!(result.contains("SUMMARY:Event")); // Default name
        assert!(result.contains("DTSTART:20240315T143000"));
        assert!(result.contains("DTEND:20240315T153000")); // Should have auto-generated end time
        assert!(result.contains("END:VEVENT"));

        // Test full event with all fields
        let full_event = IcalEvent {
            name: "Team Meeting".to_string(),
            start_time: NaiveDateTime::parse_from_str("2024-03-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            end_time: NaiveDateTime::parse_from_str("2024-03-15 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            start_time_zone: Some(chrono_tz::America::New_York),
            end_time_zone: Some(chrono_tz::America::New_York),
            location: Some("Conference Room A".to_string()),
        };

        let result = full_event.to_ics();
        assert!(result.contains("BEGIN:VEVENT"));
        assert!(result.contains("SUMMARY:Team Meeting"));
        assert!(result.contains("DTSTART:TZID=America/New_York:20240315T143000"));
        assert!(result.contains("DTEND:TZID=America/New_York:20240315T153000"));
        assert!(result.contains("LOCATION:Conference Room A"));
        assert!(result.contains("END:VEVENT"));
        assert!(result.contains("UID:")); // Should contain a UUID
        assert!(result.contains("DTSTAMP:")); // Should contain a timestamp
        assert!(result.contains("PRODID:-//UnemployedLucia//sel2ics//EN"));
    }

    #[test]
    fn calendar_response_parses_to_ical_event() {
        let cr = LlmResponseUnwrapped { ical_as_json: "{\n  \"name\": \"Underworld\",\n  \"start_time\": \"2025-05-20 20:00:00\",\n  \"start_time_zone\": \"America/Los_Angeles\",\n  \"location\": \"Showbox SoDo, Seattle, WA\",\n  \"age_restriction\": \"21 & Over\"\n}".to_owned() };
        let ical_event_json: IcalEventJson = serde_json::from_str(&cr.ical_as_json).unwrap();
        let ical_event = ical_event_json.parse().unwrap();
        assert_eq!(ical_event.name, "Underworld");
        assert_eq!(ical_event.start_time, NaiveDateTime::parse_from_str("2025-05-20 20:00:00", "%Y-%m-%d %H:%M:%S").unwrap());
        assert_eq!(ical_event.location, Some("Showbox SoDo, Seattle, WA".to_string()));
    }
}
