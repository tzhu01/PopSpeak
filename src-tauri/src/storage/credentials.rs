#[cfg(windows)]
mod platform {
    use anyhow::Result;
    use std::ptr::{null_mut, NonNull};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn save(target: &str, secret: &str) -> Result<()> {
        let mut target_wide = wide(target);
        if secret.is_empty() {
            unsafe {
                CredDeleteW(target_wide.as_mut_ptr(), CRED_TYPE_GENERIC, 0);
            }
            return Ok(());
        }

        let mut user = wide("PopSpeak");
        let mut secret_bytes = secret.as_bytes().to_vec();
        let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target_wide.as_mut_ptr();
        credential.CredentialBlobSize = secret_bytes.len() as u32;
        credential.CredentialBlob = secret_bytes.as_mut_ptr();
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        credential.UserName = user.as_mut_ptr();

        let success = unsafe { CredWriteW(&credential, 0) };
        secret_bytes.fill(0);
        if success == 0 {
            anyhow::bail!("Windows Credential Manager rejected the credential")
        }
        Ok(())
    }

    pub fn load(target: &str) -> Option<String> {
        let target_wide = wide(target);
        let mut raw: *mut CREDENTIALW = null_mut();
        let success = unsafe { CredReadW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        let pointer = NonNull::new(raw)?;
        if success == 0 {
            return None;
        }
        let credential = unsafe { pointer.as_ref() };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let value = String::from_utf8(bytes.to_vec()).ok();
        unsafe { CredFree(raw.cast()) };
        value
    }
}

#[cfg(windows)]
pub use platform::{load, save};
