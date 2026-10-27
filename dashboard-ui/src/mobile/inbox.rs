//! Platform-free rules for the server inbox: when this installation registers with the
//! server, keeps polling, or stops. The Kotlin side (`AhdInbox`) holds the registration.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What the Kotlin side reports as registered.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Registered {
    pub(crate) device: String,
    pub(crate) owner: String,
}

impl Registered {
    pub(crate) fn from_json(text: Option<&str>) -> Option<Self> {
        serde_json::from_str(text?).ok()
    }
}

/// Handed to Kotlin after a successful registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Config {
    pub(crate) origin: String,
    pub(crate) device: String,
    pub(crate) owner: String,
    pub(crate) cursor: i64,
    pub(crate) agent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Register with the server for `user`, replacing any other user's registration.
    Register(Uuid),
    /// Registered for the signed-in user: keep polling.
    Keep,
    /// Turned off: remove the server registration, then stop polling.
    Unregister(String),
    /// The registration no longer belongs here (sign-out, another user): stop locally.
    Forget,
    Idle,
}

/// `user` is the signed-in user who may receive notices (password set), if any;
/// `forgets` is a confirmed sign-out (see `reminders::forgets_schedule`).
pub(crate) fn plan(
    enabled: bool,
    user: Option<Uuid>,
    forgets: bool,
    registered: Option<&Registered>,
) -> Plan {
    let mine = |user: Uuid| registered.is_some_and(|r| r.owner == user.to_string());
    match (user, registered) {
        (Some(user), _) if enabled => {
            if mine(user) {
                Plan::Keep
            } else {
                Plan::Register(user)
            }
        }
        (Some(user), Some(registered)) if mine(user) => Plan::Unregister(registered.device.clone()),
        (Some(_), Some(_)) => Plan::Forget,
        (None, Some(_)) if forgets => Plan::Forget,
        _ => Plan::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_follows_the_switch_the_user_and_confirmed_sign_outs() {
        let user = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let mine = Registered {
            device: "d1".into(),
            owner: user.to_string(),
        };
        let theirs = Registered {
            device: "d2".into(),
            owner: other.to_string(),
        };
        assert_eq!(plan(true, Some(user), false, None), Plan::Register(user));
        assert_eq!(plan(true, Some(user), false, Some(&mine)), Plan::Keep);
        assert_eq!(
            plan(true, Some(user), false, Some(&theirs)),
            Plan::Register(user)
        );
        assert_eq!(
            plan(false, Some(user), false, Some(&mine)),
            Plan::Unregister("d1".into())
        );
        assert_eq!(plan(false, Some(user), false, Some(&theirs)), Plan::Forget);
        assert_eq!(plan(false, Some(user), false, None), Plan::Idle);
        // Signed out: only a confirmed sign-out stops polling; offline starts keep it.
        assert_eq!(plan(true, None, true, Some(&mine)), Plan::Forget);
        assert_eq!(plan(true, None, false, Some(&mine)), Plan::Idle);
        assert_eq!(plan(true, None, true, None), Plan::Idle);
        assert_eq!(
            Registered::from_json(Some(
                r#"{"device":"d1","owner":"00000000-0000-0000-0000-000000000001"}"#
            )),
            Some(mine)
        );
        assert_eq!(Registered::from_json(Some("bad")), None);
        assert_eq!(Registered::from_json(None), None);
    }
}
