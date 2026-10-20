mod dashboard;

fn main() {
    // Load environment configuration before Tokio starts worker threads.
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => fail("could not load .env"),
    }
    init_logging();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|_| fail("could not start async runtime"));
    if let Err(error) = runtime.block_on(dashboard::run()) {
        // Display intentionally omits DB driver details and secret-bearing values.
        fail(&error.to_string());
    }
}

/// `RUST_LOG` selects levels (default `info`); `LOG_FORMAT=json` emits one JSON object per line.
fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if std::env::var("LOG_FORMAT").is_ok_and(|format| format.eq_ignore_ascii_case("json")) {
        builder.json().init();
    } else {
        builder.init();
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
