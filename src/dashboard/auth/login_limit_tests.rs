use super::*;

#[test]
fn one_username_exhausting_its_budget_does_not_block_another() {
    let limiter = LoginLimiter::default();
    let now = Instant::now();
    for _ in 0..ATTEMPTS {
        limiter.admit("alice", now).unwrap();
    }
    assert!(matches!(
        limiter.admit("alice", now),
        Err(Error::RateLimited)
    ));
    assert_eq!(limiter.attempts("alice"), ATTEMPTS);
    limiter.admit("bob", now).unwrap();
    // Exact lookup: a differently cased name is a different account and bucket.
    limiter.admit("Alice", now).unwrap();
}

#[test]
fn budget_recovers_after_the_window() {
    let limiter = LoginLimiter::default();
    let start = Instant::now();
    for _ in 0..ATTEMPTS {
        limiter.admit("alice", start).unwrap();
    }
    let almost = start + WINDOW - Duration::from_millis(1);
    assert!(matches!(
        limiter.admit("alice", almost),
        Err(Error::RateLimited)
    ));
    limiter.admit("alice", start + WINDOW).unwrap();
    assert_eq!(limiter.attempts("alice"), 1);
}

#[test]
fn invalid_names_share_one_bucket() {
    let limiter = LoginLimiter::default();
    let now = Instant::now();
    for attempt in 0..ATTEMPTS {
        limiter.admit(&format!("bad name {attempt}"), now).unwrap();
    }
    assert!(matches!(
        limiter.admit(&"x".repeat(65), now),
        Err(Error::RateLimited)
    ));
    assert_eq!(limiter.0.lock().unwrap().len(), 1);
    limiter.admit("valid-name", now).unwrap();
}

#[test]
fn map_stays_bounded_by_dropping_expired_then_oldest_buckets() {
    let limiter = LoginLimiter::default();
    let start = Instant::now();
    limiter.admit("expired", start).unwrap();
    for index in 1..MAX_BUCKETS {
        limiter
            .admit(&format!("user-{index}"), start + WINDOW)
            .unwrap();
    }
    assert_eq!(limiter.0.lock().unwrap().len(), MAX_BUCKETS);
    let later = start + WINDOW + Duration::from_secs(1);
    limiter.admit("newcomer", later).unwrap();
    let buckets = limiter.0.lock().unwrap();
    assert_eq!(buckets.len(), MAX_BUCKETS);
    assert!(!buckets.contains_key("expired"));
    drop(buckets);

    // With every window live, the oldest window is evicted instead.
    limiter.admit("another", later).unwrap();
    let buckets = limiter.0.lock().unwrap();
    assert_eq!(buckets.len(), MAX_BUCKETS);
    assert!(buckets.contains_key("newcomer") && buckets.contains_key("another"));
}
