mod dashboard;

fn main() {
    // Load environment configuration before Tokio starts worker threads.
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => fail("could not load .env"),
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|_| fail("could not start async runtime"));
    if let Err(error) = runtime.block_on(dashboard::run()) {
        // Display intentionally omits DB driver details and secret-bearing values.
        fail(&error.to_string());
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
