// Read-only HoYoLAB integration check. Never claims rewards or changes privacy settings.
#[path = "../hoyolab.rs"]
mod hoyolab;

#[tokio::main]
async fn main() {
    let mut env_path = "test/.env".to_owned();
    let mut raw = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" | "-raw" => raw = true,
            "--env" | "-env" => match args.next() {
                Some(path) => env_path = path,
                None => fail("--env にはファイルパスが必要です"),
            },
            "--help" | "-h" => {
                println!("cargo run --bin login -- [--env test/.env] [--raw]");
                return;
            }
            _ => fail("不明な引数です。--help を参照してください"),
        }
    }
    match tokio::time::timeout(
        std::time::Duration::from_secs(90),
        hoyolab::run(&env_path, raw),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => fail(&err),
        Err(_) => fail("全体の制限時間90秒を超えました"),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("ERROR: {message}");
    std::process::exit(1);
}
