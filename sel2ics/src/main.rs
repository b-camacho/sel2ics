use chrono::{DateTime, Utc, TimeZone};
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

#[derive(Deserialize, Debug)]
struct CalendarRequest {
    text: String
}

#[derive(Serialize, Debug)]
struct CalendarResponse {
    ical_content: String,
}

#[derive(Serialize)]
struct LlmRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct LlmResponse {
    content: Vec<Content>,
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
    fn parse(&self) -> IcalEvent {
        use std::str::FromStr;
        IcalEvent {
            name: self.name.clone().unwrap_or("Event".to_string()),
            start_time: to_chrono(&self.start_time.clone().unwrap_or("".to_string()), &self.start_time_zone.clone().unwrap_or("America/New_York".to_string())),
            end_time: to_chrono(&self.end_time.clone().unwrap_or("".to_string()), &self.end_time_zone.clone().unwrap_or("America/New_York".to_string())),
            start_time_zone: self.start_time_zone.clone().map(|tz| chrono_tz::Tz::from_str(&tz).unwrap()),
            end_time_zone: self.end_time_zone.clone().map(|tz| chrono_tz::Tz::from_str(&tz).unwrap()),
            location: self.location.clone(),
        }
    }
}

#[derive(Debug)]
struct IcalEvent {
    name: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    start_time_zone: Option<chrono_tz::Tz>,
    end_time_zone: Option<chrono_tz::Tz>,
    location: Option<String>,
}


impl IcalEvent {
    fn to_string(&self) -> String {
        let mut output = String::new();
        output.push_str("BEGIN:VEVENT\r\n");
        output.push_str(&format!("SUMMARY:{}\r\n", self.name));
        output.push_str(&format!("DTSTART:{}\r\n", to_ical_ts(self.start_time, self.start_time_zone.clone())));
        output.push_str(&format!("DTEND:{}\r\n", to_ical_ts(self.end_time, self.end_time_zone.clone())));

        if let Some(location) = self.location.clone() {
            output.push_str(&format!("LOCATION:{}\r\n", location));
        }

        // silly protocol stuff
        output.push_str(&format!("UID:{}\r\n", Uuid::new_v4()));
        output.push_str(&format!("DTSTAMP:{}\r\n", to_ical_ts(Utc::now(), None)));
        output.push_str("PRODID:-//UnemployedLucia//sel2ics//EN\r\n");

        output.push_str("END:VEVENT\r\n");
        output
    }
}

fn to_ical_ts(t: chrono::DateTime<chrono::Utc>, z: Option<chrono_tz::Tz>) -> String {
    // if we know the tz, specify it explicitly as in DTSTART;TZID=America/New_York:20250212T150000
    // if we do not know the tz, use the "floating" format as in DTSTART:20250212T150000
    // the latter makes the calendar app use ther users's local timezone
    if let Some(tz) = z {
        // explicit timezone
        format!("TZID={}:{}", 
               tz.name(),
               t.with_timezone(&tz).format("%Y%m%dT%H%M%S"))
    } else {
        // floating time
        t.format("%Y%m%dT%H%M%S").to_string()
    }

}

// example datetime: 2022-09-27 18:00:00 (ISO8601)
// example timezone: America/New_York
fn to_chrono(datetime: &str, timezone: &str) -> DateTime<Utc> {
    use std::str::FromStr;
    let tz = Tz::from_str(timezone).unwrap();
    let naive_dt = chrono::NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S").unwrap();
    let local_dt = tz.from_local_datetime(&naive_dt).unwrap();
    local_dt.with_timezone(&Utc)
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
        messages: vec![Message {
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

async fn query_llm(ar: LlmRequest) -> Result<LlmResponse, Box<dyn std::error::Error>> {
    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
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
                return Ok(resp.json::<LlmResponse>().await?.into());
            } else {
                let error_message = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                Err(error_message.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn unwrap_llm_response(ar: LlmResponse) -> Result<CalendarResponse, String> {
    let assistant_message = &ar.content[0].text;

    if let Some(ical_content) = extract_ical_content(assistant_message) {
        let calendar_response = CalendarResponse { ical_content };
        Ok(calendar_response)
    } else {
        Err("Failed to extract iCal content".into())
    }

}

async fn route(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<String>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/cal") => {
            let body_bytes = req.collect().await.unwrap().to_bytes();
            let calendar_request: CalendarRequest = serde_json::from_slice(&body_bytes).unwrap();
            log::info!("parsed: {calendar_request:?}");
            let llm_req = build_llm_request(&calendar_request.text);
            let llm_res = query_llm(llm_req).await.unwrap();
            let user_res = unwrap_llm_response(llm_res).unwrap();
            log::info!("user_res: {user_res:?}");
            let ical_event_json: IcalEventJson = serde_json::from_str(&user_res.ical_content).unwrap();
            let ical_event = ical_event_json.parse();

            let ical_event_str = ical_event.to_string();
            log::info!("ical_event_str: {ical_event_str:?}");

            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(ical_event_str)
                .unwrap())
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not Found".to_string())
            .unwrap()),
    }
}

fn extract_ical_content(text: &str) -> Option<String> {
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
    use super::*;

    #[test]
    fn ical_event_converts_to_valid_ical_string() {
        // Test minimal event (just start time)
        let minimal_event = IcalEvent {
            name: "Event".to_string(),
            start_time: Utc.with_ymd_and_hms(2024, 3, 15, 14, 30, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2024, 3, 15, 15, 30, 0).unwrap(),
            start_time_zone: None,
            end_time_zone: None,
            location: None,
        };

        dbg!(&minimal_event);

        let result = minimal_event.to_string();
        dbg!(&result);
        assert!(result.contains("BEGIN:VEVENT"));
        assert!(result.contains("SUMMARY:Event")); // Default name
        assert!(result.contains("DTSTART:20240315T143000"));
        assert!(result.contains("DTEND:20240315T153000")); // Should have auto-generated end time
        assert!(result.contains("END:VEVENT"));

        // Test full event with all fields
        let full_event = IcalEvent {
            name: "Team Meeting".to_string(),
            start_time: to_chrono("2024-03-15 14:30:00", "America/New_York"),
            end_time: to_chrono("2024-03-15 15:30:00", "America/New_York"),
            start_time_zone: Some(chrono_tz::America::New_York),
            end_time_zone: Some(chrono_tz::America::New_York),
            location: Some("Conference Room A".to_string()),
        };

        let result = full_event.to_string();
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
}
