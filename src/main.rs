//! cargo run -- error   -> one error from the command line
//! cargo run -- panic   -> one panic
//! cargo run -- serve   -> POST /trigger on :8080 (dashboard)
use anyhow::{Context, Result};
use std::env;
use std::sync::Arc;

fn read_config(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read config {path}"))
}

fn generate(kind: &str, run: &str, fresh: bool) -> String {
    sentry::with_scope(
        |scope| {
            scope.set_tag("run_id", run);
            if fresh {
                scope.set_fingerprint(Some(["{{ default }}", run].as_slice()));
            }
        },
        || {
            if kind == "panic" {
                let r = std::panic::catch_unwind(|| {
                    let ports: Vec<u16> = vec![];
                    ports[0] // index out of bounds
                });
                assert!(r.is_err());
                return sentry::last_event_id().map(|id| id.to_string()).unwrap_or_default();
            }
            let err = read_config(&format!("/etc/sample/{run}.toml")).unwrap_err();
            sentry::integrations::anyhow::capture_anyhow(&err).to_string()
        },
    )
}

fn main() {
    // ClientOptions is #[non_exhaustive] since sentry 0.49, so no struct literal
    let mut options = sentry::ClientOptions::default();
    options.release = Some(env::var("RELEASE").unwrap_or("sample-rust@dev".into()).into());
    options.environment = Some(env::var("SENTRY_ENVIRONMENT").unwrap_or("local".into()).into());
    let _guard = sentry::init((env::var("SENTRY_DSN").ok(), options));

    let cmd = env::args().nth(1).unwrap_or("error".into());
    if cmd != "serve" {
        let run = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        println!("event id: {}", generate(&cmd, &run, true));
        return;
    }

    let server = tiny_http::Server::http("0.0.0.0:8080").unwrap();
    for mut req in server.incoming_requests() {
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let run = v["run"].as_str().unwrap_or("manual");
        let kind = v["kind"].as_str().unwrap_or("error");
        let fresh = v["fresh"].as_bool().unwrap_or(true);
        let id = match v["dsn"].as_str() {
            // dashboard "Use my DSN": separate client + hub, only this event goes there
            Some(dsn) => {
                let mut opts = sentry::ClientOptions::default();
                opts.environment = Some("playground".into());
                let opts = sentry::apply_defaults(opts);
                let client = Arc::new(sentry::Client::from_config((dsn, opts)));
                let hub = Arc::new(sentry::Hub::new(Some(client.clone()), Arc::new(Default::default())));
                let id = sentry::Hub::run(hub, || generate(kind, run, fresh));
                client.flush(Some(std::time::Duration::from_secs(5)));
                id
            }
            None => generate(kind, run, fresh),
        };
        sentry::Hub::current().client().map(|c| c.flush(Some(std::time::Duration::from_secs(5))));
        let reply = serde_json::json!({ "event_id": id }).to_string();
        let _ = req.respond(tiny_http::Response::from_string(reply));
    }
}
