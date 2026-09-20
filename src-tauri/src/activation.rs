//! Per-install offline licensing and crash-safe trial accounting.
//!
//! The public package contains only an Ed25519 verification key. A separate
//! operator tool signs installation-bound receipts; there is no master code.
//! Quotas are product friction, not DRM: an administrator can delete local data
//! or patch a binary. This module never silently resets corrupt trial storage.

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::signature::{UnparsedPublicKey, ED25519};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex, time::Duration};
use uuid::Uuid;

pub const TRIAL_RECORDINGS: u32 = 200;
pub const TRIAL_MILLISECONDS: u64 = 20 * 60 * 1000;
const DATABASE_VERSION: u32 = 2;
const DATABASE_FILE: &str = "activation.sqlite3";
const MAX_CODE_BYTES: usize = 4096;

/// Validate before any migration or abandoned-reservation cleanup. A damaged
/// ledger must not become valid merely because the new product quota is larger.
fn validate_stored_ledger(
    connection: &Connection,
    recordings_limit: u32,
    milliseconds_limit: u64,
) -> Result<()> {
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM activation_state", [], |row| {
        row.get(0)
    })?;
    if count != 1 {
        bail!("本机激活记录缺失或损坏，不会自动重置试用额度");
    }
    let (installation_id, used_recordings, used_ms): (String, u32, u64) = connection.query_row(
        "SELECT installation_id, used_recordings, used_ms FROM activation_state WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Uuid::parse_str(&installation_id).context("本机安装标识损坏，不会重置试用记录")?;
    if used_recordings > recordings_limit || used_ms > milliseconds_limit {
        bail!("本机试用记录损坏，不会重置试用额度");
    }
    let mut statement = connection.prepare("SELECT id, reserved_ms FROM trial_reservation")?;
    let reservations = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if reservations.len() > 1 {
        bail!("本机录音预留记录损坏，不会重置试用额度");
    }
    for (id, reserved_ms) in reservations {
        Uuid::parse_str(&id).context("本机录音预留标识损坏")?;
        if used_recordings == 0
            || reserved_ms == 0
            || reserved_ms > milliseconds_limit
            || reserved_ms > used_ms
        {
            bail!("本机录音预留额度损坏，不会重置试用额度");
        }
    }
    Ok(())
}

fn create_current_tables(connection: &Connection) -> Result<()> {
    connection.execute_batch(&format!(
        "CREATE TABLE activation_state (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            installation_id TEXT NOT NULL,
            used_recordings INTEGER NOT NULL CHECK(used_recordings >= 0 AND used_recordings <= {TRIAL_RECORDINGS}),
            used_ms INTEGER NOT NULL CHECK(used_ms >= 0 AND used_ms <= {TRIAL_MILLISECONDS}),
            activation_code TEXT
        );
        CREATE TABLE trial_reservation (
            id TEXT PRIMARY KEY,
            reserved_ms INTEGER NOT NULL CHECK(reserved_ms > 0 AND reserved_ms <= {TRIAL_MILLISECONDS})
        );"
    ))?;
    Ok(())
}

/// SQLite CHECK clauses cannot be changed in-place. Rebuild both v1 tables in a
/// single immediate transaction, retaining every field and the precharged
/// reservation. Callers hold the OS instance lock before entering this function.
fn initialize_or_migrate_ledger(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: u32 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        0 => {
            let existing_tables: u32 = transaction.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                 AND name IN ('activation_state', 'trial_reservation')",
                [],
                |row| row.get(0),
            )?;
            if existing_tables != 0 {
                bail!("激活记录版本缺失，不会重新创建安装标识或重置额度");
            }
            create_current_tables(&transaction)?;
            transaction.execute(
                "INSERT INTO activation_state
                 (id, installation_id, used_recordings, used_ms) VALUES (1, ?1, 0, 0)",
                [Uuid::new_v4().to_string()],
            )?;
        }
        1 => {
            validate_stored_ledger(&transaction, 30, 120_000)?;
            transaction.execute_batch(
                "ALTER TABLE activation_state RENAME TO activation_state_v1;
                 ALTER TABLE trial_reservation RENAME TO trial_reservation_v1;",
            )?;
            create_current_tables(&transaction)?;
            transaction.execute_batch(
                "INSERT INTO activation_state
                 (id, installation_id, used_recordings, used_ms, activation_code)
                 SELECT id, installation_id, used_recordings, used_ms, activation_code FROM activation_state_v1;
                 INSERT INTO trial_reservation (id, reserved_ms)
                 SELECT id, reserved_ms FROM trial_reservation_v1;
                 DROP TABLE trial_reservation_v1;
                 DROP TABLE activation_state_v1;",
            )?;
        }
        DATABASE_VERSION => {}
        _ => bail!("激活记录来自更新版本，请更新软件；不会重置试用次数"),
    }
    validate_stored_ledger(&transaction, TRIAL_RECORDINGS, TRIAL_MILLISECONDS)?;
    transaction.pragma_update(None, "user_version", DATABASE_VERSION)?;
    transaction
        .commit()
        .context("试用额度升级未完成，原有记录保持不变")?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationStatus {
    pub installation_id: String,
    pub configured: bool,
    pub activated: bool,
    pub license_id: Option<String>,
    pub trial_recordings_used: u32,
    pub trial_recordings_limit: u32,
    pub trial_milliseconds_used: u64,
    pub trial_milliseconds_limit: u64,
    pub trial_recordings_remaining: u32,
    pub trial_milliseconds_remaining: u64,
    pub trial_exhausted: bool,
    pub recording_in_progress: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingPermit {
    /// None means this recording was already authorized by a valid license.
    pub reservation_id: Option<String>,
    pub max_duration_ms: u64,
    pub activated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LicenseClaims {
    v: u32,
    product: String,
    install_id: String,
    license_id: String,
    issued_at: u64,
    features: String,
}

pub struct ActivationService {
    connection: Mutex<Option<Connection>>,
    public_key: Option<Vec<u8>>,
    startup_error: Option<String>,
    // An OS lock, not a sentinel's mere existence: automatically released on
    // process exit, including crashes. A second instance cannot clear a live
    // recording reservation as though it were an abandoned one.
    _instance_lock: Option<std::fs::File>,
}

impl ActivationService {
    /// Keep the application/history UI usable after a failed open, but grant
    /// no recognition, trial, or activation operations. No replacement DB,
    /// installation ID, or fresh quota is created (not even in memory).
    pub fn unavailable(message: String) -> Self {
        Self {
            connection: Mutex::new(None),
            public_key: None,
            startup_error: Some(message),
            _instance_lock: None,
        }
    }

    fn unavailable_error(&self) -> anyhow::Error {
        anyhow!("激活与试用记录暂不可用，识别和受限功能已暂停；历史记录仍可使用。请保留原文件并联系开发者。原因：{}",
            self.startup_error.as_deref().unwrap_or("激活数据连接不可用"))
    }

    /// Create once per process, in AppData, not alongside the portable EXE.
    /// Missing key is reported as `configured: false`, never as activated.
    pub fn open(data_dir: &Path, public_key_b64: Option<&str>) -> Result<Self> {
        let public_key = public_key_b64
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                let bytes = URL_SAFE_NO_PAD
                    .decode(value.trim())
                    .context("激活公钥格式错误")?;
                if bytes.len() != 32 {
                    bail!("激活公钥长度错误");
                }
                Ok::<Vec<u8>, anyhow::Error>(bytes)
            })
            .transpose()?;
        std::fs::create_dir_all(data_dir).context("无法创建激活数据目录")?;
        let instance_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data_dir.join("activation.lock"))
            .context("无法打开本机激活状态锁")?;
        instance_lock
            .try_lock()
            .map_err(|_| anyhow!("另一个 PopSpeak 实例正在使用本机激活记录，请先关闭该实例"))?;
        let mut connection = Connection::open(data_dir.join(DATABASE_FILE))
            .context("无法读取激活与试用记录，请保留该文件并联系开发者")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        initialize_or_migrate_ledger(&mut connection)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Quota was charged before the microphone started. A killed process
        // cannot earn more time by skipping settlement. Only clear the marker.
        transaction.execute("DELETE FROM trial_reservation", [])?;
        transaction.commit()?;
        let service = Self {
            connection: Mutex::new(Some(connection)),
            public_key,
            startup_error: None,
            _instance_lock: Some(instance_lock),
        };
        // Parse and validate storage on startup; no fallback to a new identity.
        service.status()?;
        Ok(service)
    }

    pub fn status(&self) -> Result<ActivationStatus> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow!("激活状态忙，请重新启动软件"))?;
        self.read_status(
            connection
                .as_ref()
                .ok_or_else(|| self.unavailable_error())?,
        )
    }

    fn read_status(&self, connection: &Connection) -> Result<ActivationStatus> {
        let (installation_id, used_recordings, used_ms, code): (String, u32, u64, Option<String>) =
            connection.query_row(
                "SELECT installation_id, used_recordings, used_ms, activation_code
             FROM activation_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        Uuid::parse_str(&installation_id).context("本机安装标识损坏，不会重置试用记录")?;
        if used_recordings > TRIAL_RECORDINGS || used_ms > TRIAL_MILLISECONDS {
            bail!("本机试用记录损坏，不会重置试用额度");
        }
        // A changed/corrupt receipt never grants features. Keep its quota and
        // installation ID so the user may paste a valid replacement receipt.
        let claims = code
            .as_deref()
            .and_then(|value| self.verify_code(value, &installation_id).ok());
        let activated = claims.is_some();
        let recording_in_progress = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM trial_reservation)",
            [],
            |row| row.get(0),
        )?;
        Ok(ActivationStatus {
            installation_id,
            configured: self.public_key.is_some(),
            activated,
            license_id: claims.map(|value| value.license_id),
            trial_recordings_used: used_recordings,
            trial_recordings_limit: TRIAL_RECORDINGS,
            trial_milliseconds_used: used_ms,
            trial_milliseconds_limit: TRIAL_MILLISECONDS,
            trial_recordings_remaining: TRIAL_RECORDINGS.saturating_sub(used_recordings),
            trial_milliseconds_remaining: TRIAL_MILLISECONDS.saturating_sub(used_ms),
            trial_exhausted: !activated
                && (used_recordings >= TRIAL_RECORDINGS || used_ms >= TRIAL_MILLISECONDS),
            recording_in_progress,
        })
    }

    pub fn ensure_feature(&self, feature: &str) -> Result<()> {
        let status = self.status()?;
        if status.activated || feature == "default-recognition" {
            return Ok(());
        }
        bail!("此功能需要激活。请在“激活与权益”中输入本机激活码；试用仅开放默认离线识别。")
    }

    /// Call before opening the microphone. `configured_limit_ms` is the
    /// existing technical cap, not a way to override the remaining quota.
    pub fn begin_recording(
        &self,
        provider: &str,
        configured_limit_ms: u64,
    ) -> Result<RecordingPermit> {
        if configured_limit_ms == 0 || configured_limit_ms > i64::MAX as u64 {
            bail!("录音时长上限无效");
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| anyhow!("激活状态忙，请重新启动软件"))?;
        let connection = connection
            .as_mut()
            .ok_or_else(|| self.unavailable_error())?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status = self.read_status(&transaction)?;
        if status.recording_in_progress {
            bail!("上一段试用录音尚未结束");
        }
        if status.activated {
            return Ok(RecordingPermit {
                reservation_id: None,
                max_duration_ms: configured_limit_ms,
                activated: true,
            });
        }
        if provider != "sensevoice" {
            bail!("请先激活，再使用此识别模式；试用仅开放默认离线识别。");
        }
        if status.trial_exhausted {
            bail!("免费试用已用完（累计 200 次或 20 分钟，任一先到）。请输入本机激活码继续使用。");
        }
        let max_duration_ms = configured_limit_ms.min(status.trial_milliseconds_remaining);
        let reservation_id = Uuid::new_v4().to_string();
        transaction.execute(
            "UPDATE activation_state SET used_recordings = used_recordings + 1,
             used_ms = used_ms + ?1 WHERE id = 1",
            [max_duration_ms],
        )?;
        transaction.execute(
            "INSERT INTO trial_reservation (id, reserved_ms) VALUES (?1, ?2)",
            params![reservation_id, max_duration_ms],
        )?;
        transaction
            .commit()
            .context("试用额度保存失败，录音未获授权")?;
        Ok(RecordingPermit {
            reservation_id: Some(reservation_id),
            max_duration_ms,
            activated: false,
        })
    }

    /// `actual_ms` must come from native capture frame count / a monotonic
    /// capture timer, never a frontend-supplied duration. Repeated settlement
    /// is idempotent; a failed transaction keeps the full reservation charged.
    pub fn settle_recording(&self, reservation_id: &str, actual_ms: u64) -> Result<()> {
        self.finish_reservation(reservation_id, Some(actual_ms))
    }

    /// Only for failures *before any audio capture starts*. User cancellation
    /// after capture began must call settle_recording, not this refund method.
    pub fn cancel_before_capture(&self, reservation_id: &str) -> Result<()> {
        self.finish_reservation(reservation_id, None)
    }

    fn finish_reservation(&self, reservation_id: &str, actual_ms: Option<u64>) -> Result<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| anyhow!("激活状态忙，请重新启动软件"))?;
        let connection = connection
            .as_mut()
            .ok_or_else(|| self.unavailable_error())?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let reserved_ms: Option<u64> = transaction
            .query_row(
                "SELECT reserved_ms FROM trial_reservation WHERE id = ?1",
                [reservation_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(reserved_ms) = reserved_ms else {
            return Ok(());
        };
        let charged_ms = actual_ms.unwrap_or(0).min(reserved_ms);
        let refund_count = u32::from(actual_ms.is_none());
        transaction.execute(
            "UPDATE activation_state SET used_recordings = used_recordings - ?1,
             used_ms = used_ms - ?2 WHERE id = 1",
            params![refund_count, reserved_ms - charged_ms],
        )?;
        transaction.execute(
            "DELETE FROM trial_reservation WHERE id = ?1",
            [reservation_id],
        )?;
        transaction
            .commit()
            .context("试用时长保存失败，保留已预留额度以防止重复使用")?;
        Ok(())
    }

    pub fn activate(&self, code: &str) -> Result<ActivationStatus> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| anyhow!("激活状态忙，请重新启动软件"))?;
        let connection = connection
            .as_mut()
            .ok_or_else(|| self.unavailable_error())?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status = self.read_status(&transaction)?;
        let normalized_code = code.trim();
        self.verify_code(normalized_code, &status.installation_id)?;
        transaction.execute(
            "UPDATE activation_state SET activation_code = ?1 WHERE id = 1",
            [normalized_code],
        )?;
        transaction.commit().context("激活码未能保存，尚未激活")?;
        self.read_status(connection)
    }

    fn verify_code(&self, code: &str, installation_id: &str) -> Result<LicenseClaims> {
        let public_key = self
            .public_key
            .as_ref()
            .context("当前版本尚未配置激活签名公钥，请联系开发者获取正式版本")?;
        if code.len() > MAX_CODE_BYTES {
            bail!("激活码过长");
        }
        let mut parts = code.split('.');
        if parts.next() != Some("PS1") {
            bail!("激活码格式不正确");
        }
        let payload = parts.next().context("激活码缺少内容")?;
        let signature = parts.next().context("激活码缺少签名")?;
        if parts.next().is_some() {
            bail!("激活码格式不正确");
        }
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .context("激活码签名格式不正确")?;
        // Sign the version prefix too; JSON is not reserialized before verify.
        let signed = format!("PS1.{payload}");
        UnparsedPublicKey::new(&ED25519, public_key)
            .verify(signed.as_bytes(), &signature)
            .map_err(|_| anyhow!("激活码签名无效，请完整复制，不要修改"))?;
        let claims: LicenseClaims = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(payload)
                .context("激活码内容格式不正确")?,
        )
        .context("激活码内容不正确")?;
        if claims.v != 1 || claims.product != "popspeak" || claims.features != "all_local" {
            bail!("激活码不适用于此产品或版本");
        }
        if claims.install_id != installation_id {
            bail!("激活码属于另一台安装，请按当前安装标识重新领取");
        }
        Uuid::parse_str(&claims.license_id).context("激活码授权标识无效")?;
        if claims.issued_at == 0 {
            bail!("激活码签发时间无效");
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn webviews_cannot_open_or_mutate_databases_through_sql_plugin() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        for permission in capability["permissions"].as_array().unwrap() {
            let name = permission
                .as_str()
                .unwrap_or_else(|| permission["identifier"].as_str().unwrap_or(""));
            assert!(
                !name.starts_with("sql:"),
                "Unexpected direct SQL permission: {name}"
            );
        }
        assert!(!include_str!("lib.rs").contains(".plugin(tauri_plugin_sql::"));
    }

    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn key_pair() -> Ed25519KeyPair {
        // Deterministic test-only seed. Never used by the release verifier.
        Ed25519KeyPair::from_seed_unchecked(&[23u8; 32]).unwrap()
    }

    fn open_test(path: &Path) -> ActivationService {
        let key = URL_SAFE_NO_PAD.encode(key_pair().public_key().as_ref());
        ActivationService::open(path, Some(&key)).unwrap()
    }

    fn code_for(installation_id: &str) -> String {
        let claims = LicenseClaims {
            v: 1,
            product: "popspeak".into(),
            install_id: installation_id.into(),
            license_id: Uuid::new_v4().to_string(),
            issued_at: 1_788_825_600,
            features: "all_local".into(),
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let signed = format!("PS1.{payload}");
        format!(
            "{signed}.{}",
            URL_SAFE_NO_PAD.encode(key_pair().sign(signed.as_bytes()))
        )
    }

    fn seed_v1_ledger(
        path: &Path,
        installation_id: &str,
        used_recordings: u32,
        used_ms: u64,
        activation_code: Option<&str>,
        reservation: Option<(&str, u64)>,
    ) {
        let connection = Connection::open(path.join(DATABASE_FILE)).unwrap();
        connection.execute_batch(
            "CREATE TABLE activation_state (
                id INTEGER PRIMARY KEY CHECK(id = 1), installation_id TEXT NOT NULL,
                used_recordings INTEGER NOT NULL CHECK(used_recordings >= 0 AND used_recordings <= 30),
                used_ms INTEGER NOT NULL CHECK(used_ms >= 0 AND used_ms <= 120000),
                activation_code TEXT
             );
             CREATE TABLE trial_reservation (
                id TEXT PRIMARY KEY,
                reserved_ms INTEGER NOT NULL CHECK(reserved_ms > 0 AND reserved_ms <= 120000)
             );
             PRAGMA user_version = 1;",
        ).unwrap();
        connection
            .execute(
                "INSERT INTO activation_state VALUES (1, ?1, ?2, ?3, ?4)",
                params![installation_id, used_recordings, used_ms, activation_code],
            )
            .unwrap();
        if let Some((id, milliseconds)) = reservation {
            connection
                .execute(
                    "INSERT INTO trial_reservation VALUES (?1, ?2)",
                    params![id, milliseconds],
                )
                .unwrap();
        }
    }

    #[test]
    fn new_trial_has_two_hundred_uses_and_twenty_minutes() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let status = service.status().unwrap();
        assert_eq!(status.trial_recordings_limit, 200);
        assert_eq!(status.trial_recordings_remaining, 200);
        assert_eq!(status.trial_milliseconds_limit, 1_200_000);
        assert_eq!(status.trial_milliseconds_remaining, 1_200_000);
        assert!(!status.trial_exhausted);
    }

    #[test]
    fn v1_exhausted_trial_migrates_without_resetting_usage_and_removes_old_checks() {
        let directory = tempfile::tempdir().unwrap();
        let installation_id = Uuid::new_v4().to_string();
        seed_v1_ledger(directory.path(), &installation_id, 30, 120_000, None, None);
        let service = open_test(directory.path());
        let status = service.status().unwrap();
        assert_eq!(status.installation_id, installation_id);
        assert_eq!(status.trial_recordings_used, 30);
        assert_eq!(status.trial_milliseconds_used, 120_000);
        assert_eq!(status.trial_recordings_remaining, 170);
        assert_eq!(status.trial_milliseconds_remaining, 1_080_000);
        assert!(!status.activated);
        assert!(!status.trial_exhausted);
        // Both used_recordings > 30 and reserved_ms > 120000 must now work.
        let permit = service.begin_recording("sensevoice", 300_000).unwrap();
        assert_eq!(permit.max_duration_ms, 300_000);
        service
            .settle_recording(permit.reservation_id.as_deref().unwrap(), 180_000)
            .unwrap();
        drop(service);
        let reopened = open_test(directory.path());
        let status = reopened.status().unwrap();
        assert_eq!(status.installation_id, installation_id);
        assert_eq!(status.trial_recordings_used, 31);
        assert_eq!(status.trial_milliseconds_used, 300_000);
        let connection = reopened.connection.lock().unwrap();
        let connection = connection.as_ref().unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            2
        );
        assert!(connection
            .execute("UPDATE activation_state SET used_recordings = 201", [])
            .is_err());
        assert!(connection
            .execute("UPDATE activation_state SET used_ms = 1200001", [])
            .is_err());
    }

    #[test]
    fn v1_activation_receipt_identity_and_counters_are_preserved_exactly() {
        let directory = tempfile::tempdir().unwrap();
        let installation_id = Uuid::new_v4().to_string();
        let code = code_for(&installation_id);
        seed_v1_ledger(
            directory.path(),
            &installation_id,
            7,
            21_321,
            Some(&code),
            None,
        );
        let service = open_test(directory.path());
        let status = service.status().unwrap();
        assert!(status.activated);
        assert_eq!(status.installation_id, installation_id);
        assert_eq!(status.trial_recordings_used, 7);
        assert_eq!(status.trial_milliseconds_used, 21_321);
        let saved_code: String = service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT activation_code FROM activation_state WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved_code, code);
        assert!(
            service
                .begin_recording("funasr-nano", 3_600_000)
                .unwrap()
                .activated
        );
    }

    #[test]
    fn v1_pending_reservation_is_migrated_then_crash_charge_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let installation_id = Uuid::new_v4().to_string();
        let reservation_id = Uuid::new_v4().to_string();
        seed_v1_ledger(
            directory.path(),
            &installation_id,
            8,
            100_000,
            None,
            Some((&reservation_id, 60_000)),
        );
        let mut connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
        initialize_or_migrate_ledger(&mut connection).unwrap();
        let reservation: (String, u64) = connection
            .query_row("SELECT id, reserved_ms FROM trial_reservation", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(reservation, (reservation_id.clone(), 60_000));
        drop(connection);
        // At actual startup the exited instance cannot resume capture; only
        // its marker is cleared, keeping every precharged millisecond.
        let service = open_test(directory.path());
        let status = service.status().unwrap();
        assert_eq!(status.trial_recordings_used, 8);
        assert_eq!(status.trial_milliseconds_used, 100_000);
        assert!(!status.recording_in_progress);
        service.cancel_before_capture(&reservation_id).unwrap();
        assert_eq!(service.status().unwrap().trial_milliseconds_used, 100_000);
    }

    #[test]
    fn v1_migration_failure_rolls_back_ddl_without_losing_identity_or_reservation() {
        let directory = tempfile::tempdir().unwrap();
        let installation_id = Uuid::new_v4().to_string();
        let reservation_id = Uuid::new_v4().to_string();
        seed_v1_ledger(
            directory.path(),
            &installation_id,
            8,
            100_000,
            None,
            Some((&reservation_id, 60_000)),
        );
        let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
        // Force failure after the first ALTER TABLE, proving transactional DDL rollback.
        connection
            .execute_batch("CREATE TABLE trial_reservation_v1 (sentinel TEXT);")
            .unwrap();
        drop(connection);
        assert!(ActivationService::open(directory.path(), None).is_err());
        let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            1
        );
        let saved: (String, u32, u64) = connection
            .query_row(
                "SELECT installation_id, used_recordings, used_ms FROM activation_state",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(saved, (installation_id, 8, 100_000));
        assert_eq!(
            connection
                .query_row(
                    "SELECT reserved_ms FROM trial_reservation WHERE id=?1",
                    [&reservation_id],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            60_000
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='activation_state_v1'",
                    [],
                    |row| row.get::<_, u32>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn larger_new_quota_does_not_sanitize_corrupt_v1_usage() {
        let directory = tempfile::tempdir().unwrap();
        let installation_id = Uuid::new_v4().to_string();
        seed_v1_ledger(directory.path(), &installation_id, 1, 10_000, None, None);
        let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
        connection.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE activation_state SET used_recordings = 31;").unwrap();
        drop(connection);
        assert!(ActivationService::open(directory.path(), None).is_err());
        let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT used_recordings FROM activation_state", [], |row| {
                    row.get::<_, u32>(0)
                })
                .unwrap(),
            31
        );
    }

    #[test]
    fn unknown_or_missing_version_never_resets_existing_trial() {
        for version in [0, 3] {
            let directory = tempfile::tempdir().unwrap();
            let installation_id = Uuid::new_v4().to_string();
            seed_v1_ledger(directory.path(), &installation_id, 5, 10_000, None, None);
            let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            drop(connection);
            assert!(ActivationService::open(directory.path(), None).is_err());
            let connection = Connection::open(directory.path().join(DATABASE_FILE)).unwrap();
            let saved: String = connection
                .query_row("SELECT installation_id FROM activation_state", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(saved, installation_id);
        }
    }

    #[test]
    fn signed_license_survives_restart_and_unlocks_without_resetting_counters() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let permit = service.begin_recording("sensevoice", 10_000).unwrap();
        service
            .settle_recording(permit.reservation_id.as_deref().unwrap(), 2300)
            .unwrap();
        let before = service.status().unwrap();
        assert!(service.ensure_feature("hotwords").is_err());
        assert!(service.begin_recording("funasr-nano", 10_000).is_err());
        let code = code_for(&before.installation_id);
        assert!(service.activate(&code).unwrap().activated);
        assert!(service.ensure_feature("llm-postprocessing").is_ok());
        assert!(service.ensure_feature("text-correction").is_ok());
        assert!(service.ensure_feature("hotwords").is_ok());
        let licensed = service.begin_recording("funasr-nano", 600_000).unwrap();
        assert!(licensed.activated);
        assert_eq!(licensed.max_duration_ms, 600_000);
        assert!(licensed.reservation_id.is_none());
        drop(service);
        let reopened = open_test(directory.path());
        let after = reopened.status().unwrap();
        assert!(after.activated);
        assert_eq!(after.installation_id, before.installation_id);
        assert_eq!(after.trial_recordings_used, 1);
        assert_eq!(after.trial_milliseconds_used, 2300);
    }

    #[test]
    fn invalid_codes_never_activate_or_change_identity() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let id = service.status().unwrap().installation_id;
        assert!(service
            .activate(&code_for(&Uuid::new_v4().to_string()))
            .is_err());
        assert!(service.activate("MASTER-CODE").is_err());
        let code = code_for(&id);
        let mut damaged = code.as_bytes().to_vec();
        damaged[10] = if damaged[10] == b'A' { b'B' } else { b'A' };
        assert!(service
            .activate(std::str::from_utf8(&damaged).unwrap())
            .is_err());
        let status = service.status().unwrap();
        assert!(!status.activated);
        assert_eq!(status.installation_id, id);
        assert_eq!(status.trial_recordings_used, 0);
    }

    #[test]
    fn node_crypto_signed_vector_is_accepted_by_rust_verifier() {
        // Generated independently with Node.js crypto.sign(null, message,
        // Ed25519 PKCS#8 key from the test-only [23; 32] seed).
        const CODE: &str = "PS1.eyJ2IjoxLCJwcm9kdWN0IjoicG9wc3BlYWsiLCJpbnN0YWxsX2lkIjoiMDAwMDAwMDAtMDAwMC00MDAwLTgwMDAtMDAwMDAwMDAwMDAwIiwibGljZW5zZV9pZCI6IjExMTExMTExLTExMTEtNDExMS04MTExLTExMTExMTExMTExMSIsImlzc3VlZF9hdCI6MTc4ODgyNTYwMCwiZmVhdHVyZXMiOiJhbGxfbG9jYWwifQ.Q8vi4QYTOll9E1TAjrXbJ9sof-6lLc0pllY5KjonAQDHJU1nUwuH3XckjkDG8k5ScIzY7qW4svsfpIBbfkKDBQ";
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        assert!(service
            .verify_code(CODE, "00000000-0000-4000-8000-000000000000")
            .is_ok());
        assert!(service
            .verify_code(CODE, &Uuid::new_v4().to_string())
            .is_err());
    }

    #[test]
    fn two_hundred_recordings_exhaust_trial_even_with_short_audio() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        for _ in 0..TRIAL_RECORDINGS {
            let permit = service.begin_recording("sensevoice", 10_000).unwrap();
            service
                .settle_recording(permit.reservation_id.as_deref().unwrap(), 100)
                .unwrap();
        }
        let status = service.status().unwrap();
        assert_eq!(
            status.trial_milliseconds_used,
            u64::from(TRIAL_RECORDINGS) * 100
        );
        assert_eq!(status.trial_recordings_remaining, 0);
        assert!(status.trial_exhausted);
        assert!(service.begin_recording("sensevoice", 10_000).is_err());
    }

    #[test]
    fn twenty_minutes_caps_next_recording_and_exhausts_before_count_limit() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let first = service
            .begin_recording("sensevoice", TRIAL_MILLISECONDS)
            .unwrap();
        service
            .settle_recording(
                first.reservation_id.as_deref().unwrap(),
                TRIAL_MILLISECONDS - 750,
            )
            .unwrap();
        let last = service
            .begin_recording("sensevoice", TRIAL_MILLISECONDS)
            .unwrap();
        assert_eq!(last.max_duration_ms, 750);
        service
            .settle_recording(last.reservation_id.as_deref().unwrap(), 4000)
            .unwrap();
        let status = service.status().unwrap();
        assert_eq!(status.trial_recordings_used, 2);
        assert_eq!(status.trial_milliseconds_used, TRIAL_MILLISECONDS);
        assert!(status.trial_exhausted);
    }

    #[test]
    fn crash_and_restart_preserve_reserved_quota_without_reset() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let id = service.status().unwrap().installation_id;
        service.begin_recording("sensevoice", 60_000).unwrap();
        assert!(service.begin_recording("sensevoice", 60_000).is_err());
        drop(service);
        let reopened = open_test(directory.path());
        let status = reopened.status().unwrap();
        assert_eq!(status.installation_id, id);
        assert_eq!(status.trial_recordings_used, 1);
        assert_eq!(status.trial_milliseconds_used, 60_000);
        assert!(!status.recording_in_progress);
    }

    #[test]
    fn pre_capture_failure_refunds_once_but_abort_with_audio_does_not_refund_count() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let permit = service.begin_recording("sensevoice", 60_000).unwrap();
        let id = permit.reservation_id.unwrap();
        service.cancel_before_capture(&id).unwrap();
        service.cancel_before_capture(&id).unwrap();
        assert_eq!(service.status().unwrap().trial_recordings_used, 0);
        let next = service.begin_recording("sensevoice", 60_000).unwrap();
        let next_id = next.reservation_id.unwrap();
        service.settle_recording(&next_id, 600).unwrap();
        service.settle_recording(&next_id, 0).unwrap();
        service.cancel_before_capture(&next_id).unwrap();
        let status = service.status().unwrap();
        assert_eq!(status.trial_recordings_used, 1);
        assert_eq!(status.trial_milliseconds_used, 600);
    }

    #[test]
    fn sqlite_write_failure_cannot_grant_a_recording_or_activation() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let id = service.status().unwrap().installation_id;
        service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();
        assert!(service.begin_recording("sensevoice", 60_000).is_err());
        assert!(service.activate(&code_for(&id)).is_err());
        let status = service.status().unwrap();
        assert_eq!(status.trial_recordings_used, 0);
        assert!(!status.activated);
    }

    #[test]
    fn failed_settlement_keeps_charge_until_successful_retry() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let permit = service.begin_recording("sensevoice", 60_000).unwrap();
        let id = permit.reservation_id.unwrap();
        service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();
        assert!(service.settle_recording(&id, 800).is_err());
        assert_eq!(service.status().unwrap().trial_milliseconds_used, 60_000);
        service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA query_only = OFF")
            .unwrap();
        service.settle_recording(&id, 800).unwrap();
        assert_eq!(service.status().unwrap().trial_milliseconds_used, 800);
    }

    #[test]
    fn missing_key_and_tampered_stored_receipt_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let unconfigured = ActivationService::open(directory.path(), None).unwrap();
        assert!(!unconfigured.status().unwrap().configured);
        assert!(unconfigured.activate("anything").is_err());
        drop(unconfigured);
        let service = open_test(directory.path());
        let id = service.status().unwrap().installation_id;
        service.activate(&code_for(&id)).unwrap();
        service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .execute(
                "UPDATE activation_state SET activation_code = 'PS1.invalid.invalid'",
                [],
            )
            .unwrap();
        assert!(!service.status().unwrap().activated);
        assert!(service.ensure_feature("hotwords").is_err());
    }

    #[test]
    fn corrupt_database_is_not_replaced_with_fresh_trial() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join(DATABASE_FILE);
        std::fs::write(&file, b"not a database: preserve me").unwrap();
        assert!(ActivationService::open(directory.path(), None).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"not a database: preserve me");
    }

    #[test]
    fn missing_state_row_is_not_recreated_in_existing_database() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        service
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .execute("DELETE FROM activation_state", [])
            .unwrap();
        drop(service);
        assert!(ActivationService::open(directory.path(), None).is_err());
    }

    #[test]
    fn malformed_configuration_cannot_create_usable_service() {
        let directory = tempfile::tempdir().unwrap();
        assert!(ActivationService::open(directory.path(), Some("not a public key")).is_err());
        assert!(ActivationService::open(directory.path(), Some("YWJj")).is_err());
        let service = open_test(directory.path());
        assert!(service.begin_recording("sensevoice", 0).is_err());
        assert!(service.begin_recording("sensevoice", u64::MAX).is_err());
        assert_eq!(service.status().unwrap().trial_recordings_used, 0);
    }

    #[test]
    fn second_instance_cannot_clear_an_active_reservation() {
        let directory = tempfile::tempdir().unwrap();
        let first = open_test(directory.path());
        let permit = first.begin_recording("sensevoice", 20_000).unwrap();
        assert!(ActivationService::open(directory.path(), None).is_err());
        let still_active = first.status().unwrap();
        assert!(still_active.recording_in_progress);
        assert_eq!(still_active.trial_milliseconds_used, 20_000);
        first
            .settle_recording(permit.reservation_id.as_deref().unwrap(), 1100)
            .unwrap();
        drop(first);
        let reopened = open_test(directory.path());
        assert_eq!(reopened.status().unwrap().trial_milliseconds_used, 1100);
    }

    #[test]
    fn database_timestamp_rollback_does_not_reset_trial() {
        let directory = tempfile::tempdir().unwrap();
        let service = open_test(directory.path());
        let permit = service.begin_recording("sensevoice", 30_000).unwrap();
        service
            .settle_recording(permit.reservation_id.as_deref().unwrap(), 8000)
            .unwrap();
        drop(service);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(directory.path().join(DATABASE_FILE))
            .unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(946684800)),
        )
        .unwrap();
        drop(file);
        let reopened = open_test(directory.path());
        let status = reopened.status().unwrap();
        assert_eq!(status.trial_recordings_used, 1);
        assert_eq!(status.trial_milliseconds_used, 8000);
        // Trial accounting has no local-day or wall-clock reset at all.
        assert_eq!(
            status.trial_milliseconds_remaining,
            TRIAL_MILLISECONDS - 8000
        );
    }

    #[test]
    fn unavailable_service_never_constructs_a_fresh_identity_or_grants_any_entrypoint() {
        let service = ActivationService::unavailable("原激活文件损坏".into());
        assert!(service.connection.lock().unwrap().is_none());
        assert!(service
            .status()
            .unwrap_err()
            .to_string()
            .contains("原激活文件损坏"));
        assert!(service.ensure_feature("default-recognition").is_err());
        assert!(service.ensure_feature("hotwords").is_err());
        assert!(service.begin_recording("sensevoice", 1000).is_err());
        assert!(service.activate("PS1.anything").is_err());
        assert!(service.settle_recording("old-reservation", 10).is_err());
        assert!(service.cancel_before_capture("old-reservation").is_err());
    }

    #[tokio::test]
    async fn unavailable_activation_preserves_corrupt_file_and_independent_history_access() {
        let directory = tempfile::tempdir().unwrap();
        let damaged = directory.path().join(DATABASE_FILE);
        std::fs::write(&damaged, b"preserve damaged activation bytes").unwrap();
        let history =
            crate::storage::HistoryStore::new(directory.path().join("popspeak.db")).unwrap();
        let id = history
            .add(crate::storage::HistoryEntry {
                id: 0,
                created_at: "2026-09-07T12:00:00".into(),
                app_name: "Test.exe".into(),
                app_type: "Unknown".into(),
                raw_text: "保留历史".into(),
                polished_text: "保留历史".into(),
                language: Some("zh".into()),
                duration_ms: Some(3000),
            })
            .await
            .unwrap();
        let service = match ActivationService::open(directory.path(), None) {
            Ok(_) => panic!("damaged activation DB must not open"),
            Err(error) => ActivationService::unavailable(error.to_string()),
        };
        assert!(service.begin_recording("sensevoice", 1000).is_err());
        assert_eq!(
            std::fs::read(&damaged).unwrap(),
            b"preserve damaged activation bytes"
        );
        assert_eq!(history.list(10, 0).await.unwrap()[0].raw_text, "保留历史");
        history.update_polished(id, "仍可编辑").await.unwrap();
        assert_eq!(
            history.list(10, 0).await.unwrap()[0].polished_text,
            "仍可编辑"
        );
        history.remove(id).await.unwrap();
        assert!(history.list(10, 0).await.unwrap().is_empty());
        assert_eq!(
            std::fs::read(damaged).unwrap(),
            b"preserve damaged activation bytes"
        );
    }
}
