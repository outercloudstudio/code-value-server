use axum::{
    extract::{Query, State},
    routing::{get, post},
    Router,
};
use axum::http::StatusCode;
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::vec::Vec;
use std::num::ParseIntError;
use chrono::{DateTime, TimeDelta, Utc};
use tokio;

struct LookupState {
    next_id: u64,
    lookup: HashMap<u64, String>,
    time_created: Vec<(DateTime<Utc>, u64)>,
}

async fn create(State(lookup_state): State<Arc<Mutex<LookupState>>>, value: String) -> Result<String, (StatusCode, String)> {
    let now: DateTime<Utc> = Utc::now();

    let code = {
        let mut guard = lookup_state.lock().unwrap();

        let code = (*guard).next_id;
        (*guard).next_id += 1;

        (*guard).lookup.insert(code, (&value).clone());
        (*guard).time_created.push((now, code));

        code
    };

    return Result::Ok(format!("{code}"))
}

async fn fetch(State(lookup_state): State<Arc<Mutex<LookupState>>>, Query(params): Query<HashMap<String, String>>) -> Result<String, (StatusCode, String)> {
    match params.get("code") {
        Some(code_raw) => {
            let code: Result<u64, ParseIntError> = code_raw.parse();

            match code {
                Ok(code) => {
                    let value = {
                        let guard = lookup_state.lock().unwrap();

                        match (*guard).lookup.get(&code) {
                            Some(code) => Option::Some(code.clone()),
                            None => Option::None
                        }
                    };

                    match value {
                        Some(value) => Result::Ok(value.clone()),
                        None => Result::Err((StatusCode::BAD_REQUEST, format!("Code invalid")))
                    }
                }
                _ => Result::Err((StatusCode::BAD_REQUEST, format!("Code invalid")))
            }
        },
        None => Result::Err((StatusCode::BAD_REQUEST, format!("Missing code")))
    }
}

async fn cleanup(lookup_state: Arc<Mutex<LookupState>>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let now: DateTime<Utc> = Utc::now();

        let mut guard = lookup_state.lock().unwrap();

        while (*guard).time_created.len() > 0 {
            let (created_at, code) = (*guard).time_created.get(0).unwrap().clone();

            let elapsed = now - created_at;

            if elapsed < TimeDelta::minutes(5) {
                break;
            }

            (*guard).lookup.remove(&code).unwrap();
            (*guard).time_created.remove(0);
        }
    }
}

#[tokio::main]
async fn main() {
    let lookup_state: Arc<Mutex<LookupState>> = Arc::new(Mutex::new(LookupState {
        next_id: 0,
        lookup: HashMap::new(),
        time_created: Vec::new(),
    }));

    tokio::spawn(cleanup(lookup_state.clone()));

    let app = Router::new()
        .route("/", get(fetch))
        .route("/create", post(create))
        .with_state(lookup_state);

    let config = RustlsConfig::from_pem_file("cert.pem", "key.pem")
        .await
        .unwrap();

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}