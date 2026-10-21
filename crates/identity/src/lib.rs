//! Device identity (Task 002).
//!
//! Each installation generates one Ed25519 key pair. The device ID is
//! the fingerprint of the public key, so an ID cannot be claimed
//! without the matching private key. The private key is stored in a
//! `keypair.json` file inside the device's data directory; on Unix it
//! is created with `0600` permissions, and on Windows its DACL is
//! replaced with a single full-access entry for the current user.
//! It must never be sent to the
//! coordination server or committed to source control.

use std::path::{Path, PathBuf};

use abhiverse_protocol::{DeviceInfo, DeviceKind};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hex helpers (keys and IDs travel as hex strings in the protocol).
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Device ID: first 16 bytes of SHA-256(public key), hex.
pub fn fingerprint(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    hex_encode(&digest[..16])
}

#[derive(Serialize, Deserialize)]
struct StoredKeypair {
    secret_key: String,
    public_key: String,
}

/// A loaded or freshly generated device identity.
pub struct DeviceIdentity {
    signing_key: SigningKey,
    pub info: DeviceInfo,
    /// True when this call generated a brand-new key pair.
    pub created: bool,
}

impl DeviceIdentity {
    pub fn public_key_hex(&self) -> String {
        hex_encode(self.signing_key.verifying_key().as_bytes())
    }

    pub fn sign(&self, message: &str) -> String {
        let sig = self.signing_key.sign(message.as_bytes());
        hex_encode(&sig.to_bytes())
    }
}

/// Verify an Ed25519 signature (hex) over `message` for a hex public key.
pub fn verify(public_key_hex: &str, message: &str, signature_hex: &str) -> bool {
    let Some(pk) = hex_decode(public_key_hex) else {
        return false;
    };
    let Ok(pk): Result<[u8; 32], _> = pk.try_into() else {
        return false;
    };
    let Ok(verifying) = VerifyingKey::from_bytes(&pk) else {
        return false;
    };
    let Some(sig) = hex_decode(signature_hex) else {
        return false;
    };
    let Ok(sig): Result<[u8; 64], _> = sig.try_into() else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig);
    verifying.verify(message.as_bytes(), &signature).is_ok()
}

pub fn keypair_path(data_dir: &Path) -> PathBuf {
    data_dir.join("keypair.json")
}

/// Load the identity stored in `data_dir`, generating and saving a new
/// key pair on first run.
pub fn ensure_identity(
    data_dir: &Path,
    name: &str,
    kind: DeviceKind,
) -> Result<DeviceIdentity, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let path = keypair_path(data_dir);
    if path.exists() {
        let stored: StoredKeypair = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        let secret = hex_decode(&stored.secret_key).ok_or("stored secret key is not valid hex")?;
        let secret: [u8; 32] = secret
            .try_into()
            .map_err(|_| "stored secret key has wrong length")?;
        let signing_key = SigningKey::from_bytes(&secret);
        let info = make_info(&signing_key, name, kind);
        return Ok(DeviceIdentity {
            signing_key,
            info,
            created: false,
        });
    }

    let mut rng = rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let stored = StoredKeypair {
        secret_key: hex_encode(signing_key.as_bytes()),
        public_key: hex_encode(signing_key.verifying_key().as_bytes()),
    };
    write_secret_file(&path, &serde_json::to_string_pretty(&stored)?)?;
    let info = make_info(&signing_key, name, kind);
    Ok(DeviceIdentity {
        signing_key,
        info,
        created: true,
    })
}

fn make_info(signing_key: &SigningKey, name: &str, kind: DeviceKind) -> DeviceInfo {
    let pk = signing_key.verifying_key().to_bytes();
    DeviceInfo {
        device_id: fingerprint(&pk),
        name: name.to_string(),
        platform: std::env::consts::OS.to_string(),
        kind,
        public_key: hex_encode(&pk),
    }
}

#[cfg(unix)]
fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())
}

#[cfg(windows)]
fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    // Fail closed: a keypair file we could not protect must not be
    // left behind with default (inheriting) permissions.
    if let Err(e) = restrict_to_current_user(path) {
        let _ = std::fs::remove_file(path);
        return Err(std::io::Error::other(format!(
            "could not restrict keypair file ACL: {e}"
        )));
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

/// Replace the file's DACL with a single entry: full access for the
/// current user only (the Windows analogue of Unix `0600`).
#[cfg(windows)]
fn restrict_to_current_user(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::{
        SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE,
        SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows::Win32::Security::{
        GetTokenInformation, TokenUser, ACE_FLAGS, DACL_SECURITY_INFORMATION, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;

        // First call deliberately fails and reports the buffer size.
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let filled = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        filled?;

        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: 0x001F_01FF, // FILE_ALL_ACCESS
            grfAccessMode: SET_ACCESS,
            grfInheritance: ACE_FLAGS(0),
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: PWSTR(user.User.Sid.0 as *mut u16),
            },
        };
        let mut acl: *mut windows::Win32::Security::ACL = std::ptr::null_mut();
        let r = SetEntriesInAclW(Some(&[ea]), None, &mut acl);
        if r.0 != 0 {
            return Err(format!("SetEntriesInAclW failed: {r:?}").into());
        }
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let r = SetNamedSecurityInfoW(
            PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(acl as *const windows::Win32::Security::ACL),
            None,
        );
        if !acl.is_null() {
            let _ = LocalFree(HLOCAL(acl as *mut std::ffi::c_void));
        }
        if r.0 != 0 {
            return Err(format!("SetNamedSecurityInfoW failed: {r:?}").into());
        }
        Ok(())
    }
}

/// Default per-user data directory for an agent installation.
/// Override with `ABHIVERSE_DATA_DIR`.
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ABHIVERSE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".abhiverse").join("agent");
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("Abhiverse").join("agent");
    }
    PathBuf::from(".abhiverse").join("agent")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "abhiverse-identity-test-{tag}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn identity_is_stable_across_loads() {
        let dir = temp_dir("stable");
        let _ = std::fs::remove_dir_all(&dir);
        let first = ensure_identity(&dir, "Test Laptop", DeviceKind::Agent).unwrap();
        assert!(first.created);
        let second = ensure_identity(&dir, "Test Laptop", DeviceKind::Agent).unwrap();
        assert!(!second.created);
        assert_eq!(first.info.device_id, second.info.device_id);
        assert_eq!(
            first.info.device_id,
            fingerprint(
                &hex_decode(&first.info.public_key)
                    .unwrap()
                    .try_into()
                    .unwrap()
            )
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sign_and_verify() {
        let dir = temp_dir("sign");
        let _ = std::fs::remove_dir_all(&dir);
        let id = ensure_identity(&dir, "Signer", DeviceKind::Agent).unwrap();
        let sig = id.sign("hello abhiverse");
        assert!(verify(&id.info.public_key, "hello abhiverse", &sig));
        assert!(!verify(&id.info.public_key, "tampered", &sig));
        assert!(!verify(&id.info.public_key, "hello abhiverse", "00"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn keypair_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perms");
        let _ = std::fs::remove_dir_all(&dir);
        ensure_identity(&dir, "Perms", DeviceKind::Agent).unwrap();
        let mode = std::fs::metadata(keypair_path(&dir))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
