//! Bilder vom Fingerabdruckscanner gehen verschlüsselt zum Studio-Server: AES-256-GCM mit dem
//! Schlüssel, den der Platz beim Koppeln bekommen hat. Die Auftrags-ID ist als zusätzliche
//! Angabe gebunden – ein Bild passt nur zu dem Auftrag, für den es aufgenommen wurde.
//!
//! Format: base64(nonce[12] ‖ chiffre ‖ tag[16])

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{AeadCore, Aes256Gcm, Key};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

pub fn verschluesseln(schluessel_b64: &str, auftrag: &str, daten: &[u8]) -> Result<String> {
    let roh = B64.decode(schluessel_b64).context("Bildschlüssel nicht lesbar")?;
    if roh.len() != 32 {
        return Err(anyhow!("Bildschlüssel hat nicht 32 Byte"));
    }
    let chiffre = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&roh));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut aus = nonce.to_vec();
    aus.extend(
        chiffre
            .encrypt(&nonce, Payload { msg: daten, aad: auftrag.as_bytes() })
            .map_err(|_| anyhow!("Verschlüsseln fehlgeschlagen"))?,
    );
    Ok(B64.encode(aus))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck() {
        let k = B64.encode([7u8; 32]);
        let c = verschluesseln(&k, "a1", b"bild").unwrap();
        let roh = B64.decode(c).unwrap();
        let chiffre = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&[7u8; 32]));
        let klar = chiffre
            .decrypt(aes_gcm::Nonce::from_slice(&roh[..12]), Payload { msg: &roh[12..], aad: b"a1" })
            .unwrap();
        assert_eq!(klar, b"bild");
        assert!(chiffre.decrypt(aes_gcm::Nonce::from_slice(&roh[..12]), Payload { msg: &roh[12..], aad: b"a2" }).is_err());
    }
}
