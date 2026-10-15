//! Test-only local PostgreSQL clusters and synthetic accounts; never uses DATABASE_URL.
pub(crate) mod webauthn;
use crate::dashboard::{
    auth::{
        accounts::Accounts,
        password::token_hash,
        sessions::{Identity, Sessions},
    },
    db,
};
use chrono::Duration;
use sea_orm::DatabaseConnection;
use std::{
    env, fs,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
};
use uuid::Uuid;

pub(crate) const TEMP_PASSWORD: &str = "temporary-test-password";
pub(crate) const PASSWORD: &str = "permanent-test-password";

pub(crate) struct TestDatabase {
    pub(crate) db: DatabaseConnection,
    _cluster: Cluster,
}

impl TestDatabase {
    pub(crate) async fn new() -> Self {
        let cluster = tokio::task::spawn_blocking(Cluster::start)
            .await
            .expect("start test PostgreSQL worker");
        let url = format!("postgres://attos_test@127.0.0.1:{}/postgres", cluster.port);
        let db = db::connect(&url)
            .await
            .expect("connect to isolated test PostgreSQL");
        db::migrate(&db).await.expect("apply test migrations");
        Self {
            db,
            _cluster: cluster,
        }
    }

    pub(crate) async fn ready_admin(&self) -> (Accounts, Identity) {
        let accounts = Accounts::new(self.db.clone(), Duration::days(7))
            .await
            .expect("initialize account fixture");
        accounts
            .bootstrap_admin("admin".to_owned(), TEMP_PASSWORD.to_owned())
            .await
            .expect("bootstrap synthetic admin");
        let (_, session) = accounts
            .login("admin".to_owned(), TEMP_PASSWORD.to_owned())
            .await
            .expect("login synthetic admin");
        let actor = Sessions::authenticate(
            &self.db,
            &token_hash(&session.token).expect("hash test token"),
        )
        .await
        .expect("authenticate synthetic admin");
        let (_, session) = accounts
            .change_password(&actor, Some(TEMP_PASSWORD.to_owned()), PASSWORD.to_owned())
            .await
            .expect("complete synthetic admin password setup");
        let actor = Sessions::authenticate(
            &self.db,
            &token_hash(&session.token).expect("hash rotated token"),
        )
        .await
        .expect("authenticate ready synthetic admin");
        (accounts, actor)
    }
}

struct Cluster {
    root: PathBuf,
    pg_bin: Option<PathBuf>,
    port: u16,
    started: bool,
}

impl Cluster {
    fn start() -> Self {
        let root = env::temp_dir().join(format!("attos-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create isolated test PostgreSQL directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .expect("restrict fixture directory permissions");
        }
        let port = TcpListener::bind(("127.0.0.1", 0))
            .expect("allocate loopback test port")
            .local_addr()
            .expect("read test port")
            .port();
        let mut cluster = Self {
            root,
            pg_bin: env::var_os("ATTOS_TEST_PG_BIN").map(PathBuf::from),
            port,
            started: false,
        };
        cluster.run(
            "initdb",
            &[
                "-D",
                cluster.data().to_str().expect("UTF-8 fixture path"),
                "-U",
                "attos_test",
                "--auth=trust",
                "--no-locale",
                "--encoding=UTF8",
            ],
        );
        let options = format!(
            "-h 127.0.0.1 -p {} -k {} -c max_connections=20 -c shared_buffers=8MB -c fsync=off",
            port,
            cluster.root.display()
        );
        cluster.started = true;
        cluster.run(
            "pg_ctl",
            &[
                "-D",
                cluster.data().to_str().expect("UTF-8 fixture path"),
                "-l",
                cluster
                    .root
                    .join("postgres.log")
                    .to_str()
                    .expect("UTF-8 fixture log"),
                "-o",
                &options,
                "-t",
                "15",
                "-w",
                "start",
            ],
        );
        cluster
    }

    fn data(&self) -> PathBuf {
        self.root.join("data")
    }
    fn tool(&self, name: &str) -> PathBuf {
        self.pg_bin
            .as_ref()
            .map_or_else(|| PathBuf::from(name), |p| p.join(name))
    }
    fn run(&self, tool: &str, args: &[&str]) {
        let status = Command::new(self.tool(tool)).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap_or_else(|_| panic!("cannot run {tool}; install PostgreSQL tools or set ATTOS_TEST_PG_BIN (never uses a live database)"));
        assert!(
            status.success(),
            "isolated PostgreSQL {tool} failed; fixtures require a non-root user and local PostgreSQL tools"
        );
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        if self.started && self.data().join("postmaster.pid").exists() {
            let stopped = Command::new(self.tool("pg_ctl"))
                .args(["-D"])
                .arg(self.data())
                .args(["-m", "immediate", "-t", "15", "-w", "stop"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !stopped {
                eprintln!(
                    "isolated PostgreSQL cleanup failed; fixture directory retained at {}",
                    self.root.display()
                );
                return;
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
