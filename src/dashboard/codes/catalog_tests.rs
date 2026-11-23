use super::*;
use crate::dashboard::{codes::source::SourceCode, test_support::TestDatabase};
use sea_orm::QueryOrder;

fn listing(codes: &[(&str, &str)]) -> Listing {
    Listing {
        codes: codes
            .iter()
            .map(|(code, rewards)| SourceCode {
                code: (*code).to_owned(),
                rewards: (*rewards).to_owned(),
            })
            .collect(),
        rejected: 0,
    }
}

async fn rows(db: &DatabaseConnection, game: &str) -> Vec<redemption_code::Model> {
    redemption_code::Entity::find()
        .filter(redemption_code::Column::GameKind.eq(game))
        .order_by_asc(redemption_code::Column::Code)
        .all(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn listings_add_refresh_and_unlist_codes_without_ever_deleting_on_their_own() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    let start = Utc::now();
    // The first listing of a game seeds the catalogue: those codes predate AHD watching.
    assert_eq!(
        store(
            db,
            "genshin",
            &listing(&[("OLDCODE1", "Primogem*60"), ("OLDCODE2", "")]),
            start,
            true,
        )
        .await
        .unwrap(),
        2
    );
    let first = rows(db, "genshin").await;
    assert!(first.iter().all(|row| row.seeded && row.listed));
    assert_eq!(first[0].first_seen_at, first[0].last_seen_at);

    // Later: one code stays (its empty text never hides a known one), one disappears and
    // one is new.
    let later = start + Duration::hours(3);
    assert_eq!(
        store(
            db,
            "genshin",
            &listing(&[("OLDCODE1", ""), ("NEWCODE1", "Mora*10000")]),
            later,
            false,
        )
        .await
        .unwrap(),
        1
    );
    let second = rows(db, "genshin").await;
    let by_code = |code: &str| second.iter().find(|row| row.code == code).unwrap().clone();
    let kept = by_code("OLDCODE1");
    assert_eq!(kept.rewards, "Primogem*60");
    assert!(kept.listed);
    assert_eq!(kept.last_seen_at.timestamp(), later.timestamp());
    assert_eq!(kept.first_seen_at.timestamp(), start.timestamp());
    let gone = by_code("OLDCODE2");
    assert!(!gone.listed);
    assert_eq!(gone.last_seen_at.timestamp(), start.timestamp());
    let new = by_code("NEWCODE1");
    assert!(new.listed && !new.seeded);
    assert_eq!(new.first_seen_at.timestamp(), later.timestamp());

    // A code that comes back is listed again under its first sighting.
    store(
        db,
        "genshin",
        &listing(&[("OLDCODE2", "")]),
        later + Duration::hours(3),
        false,
    )
    .await
    .unwrap();
    let back = rows(db, "genshin").await;
    assert!(
        back.iter()
            .find(|row| row.code == "OLDCODE2")
            .unwrap()
            .listed
    );
    assert!(
        !back
            .iter()
            .find(|row| row.code == "NEWCODE1")
            .unwrap()
            .listed
    );

    // Games are independent: Star Rail's first listing is its own seed.
    store(
        db,
        "starrail",
        &listing(&[("STARRAILGIFT", "50 stellar jade")]),
        later,
        true,
    )
    .await
    .unwrap();
    assert!(rows(db, "starrail").await[0].seeded);
    assert_eq!(rows(db, "genshin").await.len(), 3);

    // Only codes unlisted for 90 days are pruned (OLDCODE1 and NEWCODE1, last seen `later`);
    // the listed OLDCODE2 stays however old its first sighting is.
    assert_eq!(prune(db, later + Duration::days(89)).await.unwrap(), 0);
    assert_eq!(prune(db, later + Duration::days(91)).await.unwrap(), 2);
    let left: Vec<_> = rows(db, "genshin")
        .await
        .into_iter()
        .map(|row| row.code)
        .collect();
    assert_eq!(left, ["OLDCODE2"]);

    // Once a run has succeeded, a game with no codes left takes new ones as news.
    redemption_code::Entity::delete_many()
        .filter(redemption_code::Column::GameKind.eq("starrail"))
        .exec(db)
        .await
        .unwrap();
    store(db, "starrail", &listing(&[("RAILNEWS1", "")]), later, false)
        .await
        .unwrap();
    assert!(!rows(db, "starrail").await[0].seeded);
}

#[tokio::test]
async fn the_schema_rejects_codes_and_texts_the_parser_would_never_produce() {
    let fixture = TestDatabase::new().await;
    let db = &fixture.db;
    for (code, rewards) in [
        ("lowercase", ""),
        ("ABC", ""),
        ("WITH SPACE", ""),
        ("GOODCODE", &*"x".repeat(301)),
    ] {
        let result = store(
            db,
            "genshin",
            &listing(&[(code, rewards)]),
            Utc::now(),
            true,
        )
        .await;
        assert!(result.is_err(), "{code}");
    }
    assert!(
        store(db, "nap", &listing(&[("GOODCODE", "")]), Utc::now(), true)
            .await
            .is_err()
    );
    assert!(
        rows(db, "genshin").await.is_empty(),
        "a failed listing wrote rows"
    );
}
