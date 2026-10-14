use openssl::{
    asn1::Asn1Time,
    bn::{BigNum, MsbOption},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    rsa::Rsa,
    ssl::{NameType, SslAcceptor, SslMethod},
    x509::{
        X509, X509NameBuilder,
        extension::{BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAlternativeName},
    },
};
use reqwest::{
    Certificate, Client,
    header::{CONNECTION, COOKIE, HeaderMap, HeaderValue, USER_AGENT},
};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

const HOSTS: [&str; 5] = [
    "api-os-takumi.mihoyo.com",
    "api-account-os.hoyolab.com",
    "sg-public-api.hoyolab.com",
    "sg-hk4e-api.hoyolab.com",
    "bbs-api-os.hoyolab.com",
];
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 8 * 1024;

pub struct TlsMockServer {
    address: SocketAddr,
    cert_pem: Vec<u8>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    expected: VecDeque<ExpectedRequest>,
    errors: Vec<String>,
}

#[derive(Debug)]
pub struct ExpectedRequest {
    method: String,
    host: String,
    target: String,
    required_headers: Vec<(String, String)>,
    present_headers: Vec<String>,
    absent_headers: Vec<String>,
    status: u16,
    body: Vec<u8>,
}

struct Request {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    sni: Option<String>,
}

impl ExpectedRequest {
    pub fn get(host: &str, target: &str, body: String) -> Self {
        Self::new("GET", host, target, 200, body.into_bytes())
    }

    pub fn post(host: &str, target: &str, body: String) -> Self {
        Self::new("POST", host, target, 200, body.into_bytes())
    }

    pub fn status(host: &str, target: &str, status: u16) -> Self {
        Self::new("GET", host, target, status, b"{}".to_vec())
    }

    pub fn large(host: &str, target: &str, bytes: usize) -> Self {
        Self::new("GET", host, target, 200, vec![b'x'; bytes])
    }

    fn new(method: &str, host: &str, target: &str, status: u16, body: Vec<u8>) -> Self {
        Self {
            method: method.to_owned(),
            host: host.to_owned(),
            target: target.to_owned(),
            required_headers: Vec::new(),
            present_headers: Vec::new(),
            absent_headers: Vec::new(),
            status,
            body,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.required_headers
            .push((name.to_ascii_lowercase(), value.to_owned()));
        self
    }

    pub fn present_header(mut self, name: &str) -> Self {
        self.present_headers.push(name.to_ascii_lowercase());
        self
    }

    pub fn absent_header(mut self, name: &str) -> Self {
        self.absent_headers.push(name.to_ascii_lowercase());
        self
    }
}

impl TlsMockServer {
    pub fn start(expected: Vec<ExpectedRequest>) -> Self {
        let (cert, key, cert_pem) = certificate();
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_private_key(&key).unwrap();
        acceptor.set_certificate(&cert).unwrap();
        acceptor.set_servername_callback(|_, _| Ok(()));
        acceptor.check_private_key().unwrap();
        let acceptor = Arc::new(acceptor.build());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(State {
            expected: expected.into(),
            errors: Vec::new(),
        }));
        let thread_state = Arc::clone(&state);
        let thread = thread::spawn(move || serve(listener, acceptor, stop_rx, thread_state));

        Self {
            address,
            cert_pem,
            stop: Some(stop_tx),
            thread: Some(thread),
            state,
        }
    }

    pub fn client(&self, cookie: &str, user_agent: &str) -> Client {
        let mut headers = HeaderMap::new();
        let mut cookie = HeaderValue::from_str(cookie).unwrap();
        cookie.set_sensitive(true);
        headers.insert(COOKIE, cookie);
        headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap());
        headers.insert(CONNECTION, HeaderValue::from_static("close"));

        let mut builder = Client::builder()
            .default_headers(headers)
            .add_root_certificate(Certificate::from_pem(&self.cert_pem).unwrap())
            .tls_built_in_root_certs(false)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .http1_only();
        for host in HOSTS {
            builder = builder.resolve(host, self.address);
        }
        builder.build().unwrap()
    }

    pub fn assert_finished(mut self) {
        self.stop_and_join();
        let state = self.state.lock().unwrap();
        assert!(
            state.errors.is_empty(),
            "TLS mock errors: {:#?}",
            state.errors
        );
        assert!(
            state.expected.is_empty(),
            "TLS mock did not receive {} expected request(s)",
            state.expected.len()
        );
    }

    fn stop_and_join(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            self.state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .errors
                .push("TLS mock worker panicked".to_owned());
        }
    }
}

impl Drop for TlsMockServer {
    fn drop(&mut self) {
        self.stop_and_join();
        if thread::panicking() {
            let state = self.state.lock().unwrap();
            if !state.errors.is_empty() || !state.expected.is_empty() {
                eprintln!(
                    "TLS mock dropped during panic; errors={:#?}; remaining={}",
                    state.errors,
                    state.expected.len()
                );
            }
        }
    }
}

fn certificate() -> (X509, PKey<Private>, Vec<u8>) {
    let root_key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut root_name = X509NameBuilder::new().unwrap();
    root_name
        .append_entry_by_nid(Nid::COMMONNAME, "hoyolab-test-root")
        .unwrap();
    let root_name = root_name.build();
    let root_serial = serial_number();
    let not_before = Asn1Time::days_from_now(0).unwrap();
    let not_after = Asn1Time::days_from_now(1).unwrap();

    let mut root = X509::builder().unwrap();
    root.set_version(2).unwrap();
    root.set_serial_number(&root_serial).unwrap();
    root.set_subject_name(&root_name).unwrap();
    root.set_issuer_name(&root_name).unwrap();
    root.set_pubkey(&root_key).unwrap();
    root.set_not_before(&not_before).unwrap();
    root.set_not_after(&not_after).unwrap();
    root.append_extension(BasicConstraints::new().critical().ca().build().unwrap())
        .unwrap();
    root.append_extension(KeyUsage::new().key_cert_sign().crl_sign().build().unwrap())
        .unwrap();
    root.sign(&root_key, MessageDigest::sha256()).unwrap();
    let root = root.build();

    let leaf_key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut leaf_name = X509NameBuilder::new().unwrap();
    leaf_name
        .append_entry_by_nid(Nid::COMMONNAME, HOSTS[0])
        .unwrap();
    let leaf_name = leaf_name.build();
    let leaf_serial = serial_number();
    let mut leaf = X509::builder().unwrap();
    leaf.set_version(2).unwrap();
    leaf.set_serial_number(&leaf_serial).unwrap();
    leaf.set_subject_name(&leaf_name).unwrap();
    leaf.set_issuer_name(root.subject_name()).unwrap();
    leaf.set_pubkey(&leaf_key).unwrap();
    leaf.set_not_before(&not_before).unwrap();
    leaf.set_not_after(&not_after).unwrap();
    leaf.append_extension(BasicConstraints::new().critical().build().unwrap())
        .unwrap();
    leaf.append_extension(
        KeyUsage::new()
            .digital_signature()
            .key_encipherment()
            .build()
            .unwrap(),
    )
    .unwrap();
    leaf.append_extension(ExtendedKeyUsage::new().server_auth().build().unwrap())
        .unwrap();
    let mut san = SubjectAlternativeName::new();
    for host in HOSTS {
        san.dns(host);
    }
    let san = san.build(&leaf.x509v3_context(Some(&root), None)).unwrap();
    leaf.append_extension(san).unwrap();
    leaf.sign(&root_key, MessageDigest::sha256()).unwrap();
    (leaf.build(), leaf_key, root.to_pem().unwrap())
}

fn serial_number() -> openssl::asn1::Asn1Integer {
    let mut serial = BigNum::new().unwrap();
    serial.rand(128, MsbOption::MAYBE_ZERO, false).unwrap();
    serial.to_asn1_integer().unwrap()
}

fn serve(
    listener: TcpListener,
    acceptor: Arc<SslAcceptor>,
    stop: mpsc::Receiver<()>,
    state: Arc<Mutex<State>>,
) {
    loop {
        if stop.try_recv().is_ok() {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => handle_connection(stream, &acceptor, &state),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                state
                    .lock()
                    .unwrap()
                    .errors
                    .push(format!("accept failed: {error}"));
                break;
            }
        }
    }
}

fn handle_connection(stream: TcpStream, acceptor: &SslAcceptor, state: &Mutex<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut stream = match acceptor.accept(stream) {
        Ok(stream) => stream,
        Err(error) => {
            state
                .lock()
                .unwrap()
                .errors
                .push(format!("TLS accept failed: {error}"));
            return;
        }
    };
    let sni = stream
        .ssl()
        .servername(NameType::HOST_NAME)
        .map(ToOwned::to_owned);
    let request = match read_request(&mut stream, sni) {
        Ok(request) => request,
        Err(error) => {
            state.lock().unwrap().errors.push(error);
            let _ = write_response(&mut stream, 500, b"{}".as_slice());
            return;
        }
    };
    let expected = state.lock().unwrap().expected.pop_front();
    let Some(expected) = expected else {
        state.lock().unwrap().errors.push(format!(
            "unexpected request: {} {}",
            request.method, request.target
        ));
        let _ = write_response(&mut stream, 500, b"{}".as_slice());
        return;
    };

    let mut errors = compare(&expected, &request);
    let status = if errors.is_empty() {
        expected.status
    } else {
        500
    };
    if !errors.is_empty() {
        state.lock().unwrap().errors.append(&mut errors);
    }
    let _ = write_response(&mut stream, status, &expected.body);
}

fn read_request(
    stream: &mut openssl::ssl::SslStream<TcpStream>,
    sni: Option<String>,
) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let mut header_bytes = 0;
    let request_line = read_line(&mut reader, &mut header_bytes)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?.to_owned();
    let target = parts.next().ok_or("missing HTTP target")?.to_owned();
    let version = parts.next().ok_or("missing HTTP version")?;
    if version != "HTTP/1.1" {
        return Err(format!("unexpected HTTP version: {version}"));
    }

    let mut headers = HashMap::new();
    loop {
        let line = read_line(&mut reader, &mut header_bytes)?;
        if line.is_empty() {
            break;
        }
        if headers.len() >= 64 {
            return Err("too many request headers".to_owned());
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("malformed header: {line}"))?;
        headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
    }

    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "invalid content-length".to_owned())?
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(format!("request body too large: {content_length}"));
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("body read failed: {error}"))?;

    Ok(Request {
        method,
        target,
        headers,
        body,
        sni,
    })
}

fn read_line<R: BufRead>(reader: &mut R, total: &mut usize) -> Result<String, String> {
    let mut line = String::new();
    let bytes = reader
        .take((MAX_HEADER_BYTES.saturating_sub(*total) + 1) as u64)
        .read_line(&mut line)
        .map_err(|error| format!("request read failed: {error}"))?;
    if bytes == 0 {
        return Err("connection closed while reading request".to_owned());
    }
    *total += bytes;
    if *total > MAX_HEADER_BYTES {
        return Err("request header too large".to_owned());
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

fn compare(expected: &ExpectedRequest, request: &Request) -> Vec<String> {
    let mut errors = Vec::new();
    if request.method != expected.method {
        errors.push(format!(
            "method mismatch: expected {}, got {}",
            expected.method, request.method
        ));
    }
    if request.target != expected.target {
        errors.push(format!(
            "target mismatch: expected {}, got {}",
            expected.target, request.target
        ));
    }
    if request.sni.as_deref() != Some(expected.host.as_str()) {
        errors.push(format!(
            "SNI mismatch: expected {}, got {:?}",
            expected.host, request.sni
        ));
    }
    match request.headers.get("host") {
        Some(host) if host == &expected.host => {}
        other => errors.push(format!(
            "Host header mismatch: expected {}, got {:?}",
            expected.host, other
        )),
    }
    if !request.body.is_empty() {
        errors.push(format!(
            "expected empty body, got {} bytes",
            request.body.len()
        ));
    }
    for (name, value) in &expected.required_headers {
        match request.headers.get(name) {
            Some(actual) if actual == value => {}
            other => errors.push(format!(
                "header {name} mismatch: expected {value:?}, got {other:?}"
            )),
        }
    }
    for name in &expected.present_headers {
        if !request.headers.contains_key(name) {
            errors.push(format!("missing header {name}"));
        }
    }
    for name in &expected.absent_headers {
        if request.headers.contains_key(name) {
            errors.push(format!("unexpected header {name}"));
        }
    }
    errors
}

fn write_response(
    stream: &mut openssl::ssl::SslStream<TcpStream>,
    status: u16,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        307 => "Temporary Redirect",
        500 => "Internal Server Error",
        _ => "Mock",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
        body.len()
    )?;
    if status == 307 {
        write!(
            stream,
            "location: /event/game_record/genshin/api/index?role_id=7&server=test_server\r\n"
        )?;
    }
    write!(stream, "\r\n")?;
    stream.write_all(body)?;
    stream.flush()
}
