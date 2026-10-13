//! Software-generated WebAuthn transcripts for production-library verification tests.
//! The UV flag is synthetic: these fixtures are not devices or evidence of user verification.
use crate::dashboard::auth::passkeys::{AuthenticationChallenge, RegistrationChallenge};
use openssl::{
    bn::{BigNum, BigNumContext},
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    sign::Signer,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use webauthn_rs::prelude::{Base64UrlSafeData, PublicKeyCredential, RegisterPublicKeyCredential};

pub(in crate::dashboard) const RP_ID: &str = "localhost";
pub(in crate::dashboard) const ORIGIN: &str = "http://localhost:8080";

pub(in crate::dashboard) struct TestAuthenticator {
    key: PKey<Private>,
    credential_id: Vec<u8>,
    user_handle: Value,
}

impl TestAuthenticator {
    pub(in crate::dashboard) fn new() -> Self {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        let key = PKey::from_ec_key(EcKey::generate(&group).unwrap()).unwrap();
        let mut credential_id = vec![0; 32];
        openssl::rand::rand_bytes(&mut credential_id).unwrap();
        Self {
            key,
            credential_id,
            user_handle: Value::Null,
        }
    }

    pub(in crate::dashboard) fn register(
        &mut self,
        challenge: &RegistrationChallenge,
        counter: u32,
    ) -> RegisterPublicKeyCredential {
        let options = serde_json::to_value(&challenge.options).unwrap();
        self.user_handle = options["publicKey"]["user"]["id"].clone();
        let client = client_data("webauthn.create", &options, ORIGIN);
        let mut auth = authenticator_data(0x45, counter);
        auth.extend_from_slice(&[0; 16]); // Unspecified AAGUID for none attestation.
        auth.extend_from_slice(
            &u16::try_from(self.credential_id.len())
                .unwrap()
                .to_be_bytes(),
        );
        auth.extend_from_slice(&self.credential_id);
        let ec = self.key.ec_key().unwrap();
        let mut context = BigNumContext::new().unwrap();
        let mut x = BigNum::new().unwrap();
        let mut y = BigNum::new().unwrap();
        ec.public_key()
            .affine_coordinates_gfp(ec.group(), &mut x, &mut y, &mut context)
            .unwrap();
        // Fixed COSE EC2 map: kty=2, alg=-7, crv=1, x and y each 32 bytes.
        auth.extend_from_slice(&[0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21]);
        cbor_bytes(&mut auth, &x.to_vec_padded(32).unwrap());
        auth.push(0x22);
        cbor_bytes(&mut auth, &y.to_vec_padded(32).unwrap());
        // Fixed CBOR map {fmt:"none", attStmt:{}, authData:bytes}.
        let mut attestation = b"\xa3\x63fmt\x64none\x67attStmt\xa0\x68authData".to_vec();
        cbor_bytes(&mut attestation, &auth);
        serde_json::from_value(json!({
            "id": encoded(&self.credential_id), "rawId": encoded(&self.credential_id),
            "type": "public-key", "extensions": {},
            "response": {"clientDataJSON": encoded(&client),
                "attestationObject": encoded(&attestation), "transports": ["internal"]}
        }))
        .unwrap()
    }

    pub(in crate::dashboard) fn authenticate(
        &self,
        challenge: &AuthenticationChallenge,
        counter: u32,
    ) -> PublicKeyCredential {
        self.authenticate_at_origin(challenge, counter, ORIGIN)
    }

    /// The alternate origin is included before signing, so rejection tests use valid signatures.
    pub(in crate::dashboard) fn authenticate_at_origin(
        &self,
        challenge: &AuthenticationChallenge,
        counter: u32,
        origin: &str,
    ) -> PublicKeyCredential {
        let options = serde_json::to_value(&challenge.options).unwrap();
        let client = client_data("webauthn.get", &options, origin);
        let auth = authenticator_data(0x05, counter);
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key).unwrap();
        signer.update(&auth).unwrap();
        signer.update(&Sha256::digest(&client)).unwrap();
        let signature = signer.sign_to_vec().unwrap();
        serde_json::from_value(json!({
            "id": encoded(&self.credential_id), "rawId": encoded(&self.credential_id),
            "type": "public-key", "extensions": {},
            "response": {"clientDataJSON": encoded(&client), "authenticatorData": encoded(&auth),
                "signature": encoded(&signature), "userHandle": self.user_handle}
        }))
        .unwrap()
    }
}

fn encoded(bytes: &[u8]) -> Value {
    serde_json::to_value(Base64UrlSafeData::from(bytes.to_vec())).unwrap()
}

fn client_data(kind: &str, options: &Value, origin: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"type": kind, "challenge": options["publicKey"]["challenge"],
        "origin": origin, "crossOrigin": false}),
    )
    .unwrap()
}

fn authenticator_data(flags: u8, counter: u32) -> Vec<u8> {
    let mut bytes = Sha256::digest(RP_ID.as_bytes()).to_vec();
    bytes.push(flags);
    bytes.extend_from_slice(&counter.to_be_bytes());
    bytes
}

fn cbor_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    match bytes.len() {
        0..=23 => output.push(0x40 | u8::try_from(bytes.len()).unwrap()),
        24..=255 => output.extend_from_slice(&[0x58, u8::try_from(bytes.len()).unwrap()]),
        _ => {
            output.push(0x59);
            output.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
        }
    }
    output.extend_from_slice(bytes);
}
