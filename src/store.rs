use anyhow::Result;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct PackageRow {
    pub id: i64,
    pub slug: String,
    pub source_url: String,
    pub current_sha256: Option<String>,
    pub status: String,
    pub first_seen_at: String,
    pub installed_at: Option<String>,
    pub last_ran_at: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct Invocation {
    pub id: String,
    pub package_id: Option<i64>,
    pub ts_started: String,
    pub ts_finished: Option<String>,
    pub raw_input: String,
    pub url: Option<String>,
    pub final_url: Option<String>,
    pub sha256: Option<String>,
    pub install_command_json: Option<String>,
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
}
pub struct Store {
    home: PathBuf,
    db: rusqlite::Connection,
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
impl Store {
    pub fn open(home: &Path) -> Result<Self> {
        let created = !home.exists();
        std::fs::create_dir_all(home)?;
        if created {
            private(home, 0o700)?;
        }
        let db_path = home.join("sweep.db");
        let db = rusqlite::Connection::open(&db_path)?;
        // Raw commands and their secrets live here; keep them out of other users' reach.
        private(&db_path, 0o600)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
 CREATE TABLE IF NOT EXISTS schema_meta(version INTEGER PRIMARY KEY);
 CREATE TABLE IF NOT EXISTS packages(id INTEGER PRIMARY KEY,slug TEXT NOT NULL,source_url TEXT NOT NULL UNIQUE,current_sha256 TEXT,status TEXT NOT NULL,first_seen_at TEXT NOT NULL,installed_at TEXT,last_ran_at TEXT);
 CREATE TABLE IF NOT EXISTS invocations(id TEXT PRIMARY KEY,package_id INTEGER REFERENCES packages(id),ts_started TEXT NOT NULL,ts_finished TEXT,raw_input TEXT NOT NULL,url TEXT,final_url TEXT,sha256 TEXT,install_command_json TEXT,outcome TEXT NOT NULL,exit_code INTEGER,error_message TEXT);
 CREATE INDEX IF NOT EXISTS packages_sha ON packages(current_sha256);CREATE INDEX IF NOT EXISTS invocations_pkg ON invocations(package_id);CREATE INDEX IF NOT EXISTS invocations_ts ON invocations(ts_started DESC);INSERT OR IGNORE INTO schema_meta(version) VALUES(1);")?;
        Ok(Self {
            home: home.into(),
            db,
        })
    }
    pub fn find_or_create_package(&self, url: &str, slug: &str) -> Result<PackageRow> {
        self.db.execute("INSERT INTO packages(slug,source_url,status,first_seen_at) VALUES(?1,?2,'attempting',?3) ON CONFLICT(source_url) DO NOTHING",rusqlite::params![slug,url,now()])?;
        Ok(self.db.query_row(
            "SELECT * FROM packages WHERE source_url=?1",
            [url],
            package_row,
        )?)
    }
    pub fn list_installed_packages(&self) -> Result<Vec<PackageRow>> {
        let mut stmt = self.db.prepare(
            "SELECT * FROM packages WHERE status='installed' ORDER BY installed_at DESC",
        )?;
        Ok(stmt
            .query_map([], package_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn insert_invocation(&self, inv: &Invocation) -> Result<()> {
        insert(&self.db, inv)
    }
    /// Publish the package and unfinished invocation together, before spawning.
    pub fn begin_exec(&mut self, inv: &mut Invocation, url: &str, slug: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO packages(slug,source_url,status,first_seen_at) VALUES(?1,?2,'attempting',?3) ON CONFLICT(source_url) DO NOTHING", rusqlite::params![slug, url, now()])?;
        let package_id = tx.query_row(
            "SELECT id FROM packages WHERE source_url=?1",
            [url],
            |row| row.get(0),
        )?;
        let mut pending = inv.clone();
        pending.package_id = Some(package_id);
        insert(&tx, &pending)?;
        tx.commit()?;
        inv.package_id = Some(package_id);
        Ok(())
    }
    pub fn record_exec(
        &mut self,
        inv: &Invocation,
        package_id: i64,
        sha: &str,
        exit_code: i32,
        ran_at: &str,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("UPDATE packages SET status=CASE WHEN ?1=0 THEN 'installed' WHEN status='attempting' THEN 'failed' ELSE status END,current_sha256=CASE WHEN ?1=0 THEN ?2 ELSE current_sha256 END,installed_at=CASE WHEN ?1=0 AND installed_at IS NULL THEN ?3 ELSE installed_at END,last_ran_at=?3 WHERE id=?4",rusqlite::params![exit_code,sha,ran_at,package_id])?;
        let updated = tx.execute("UPDATE invocations SET ts_finished=?1,outcome=?2,exit_code=?3,error_message=?4 WHERE id=?5 AND outcome='running' AND ts_finished IS NULL AND package_id=?6", rusqlite::params![inv.ts_finished,inv.outcome,inv.exit_code,inv.error_message,inv.id,package_id])?;
        if updated == 0 {
            insert(&tx, inv)?;
        }
        tx.commit()?;
        Ok(())
    }
    fn script_path(&self, sha: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            sha.len() == 64 && sha.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid script SHA-256"
        );
        Ok(self.home.join("cache/scripts").join(sha))
    }
    pub fn save_script(&self, sha: &str, bytes: &[u8]) -> Result<PathBuf> {
        use std::io::Write;
        let path = self.script_path(sha)?;
        let parent = path.parent().expect("script parent");
        std::fs::create_dir_all(parent)?;
        // Publish only complete bytes, including under concurrent writers.
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        match temp.persist_noclobber(&path) {
            Ok(_) => {}
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.error.into()),
        }
        Ok(path)
    }
    pub fn read_script(&self, sha: &str) -> Result<Option<Vec<u8>>> {
        match std::fs::read(self.script_path(sha)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
#[cfg(unix)]
fn private(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(mode),
    )?)
}
#[cfg(not(unix))]
fn private(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}
fn package_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PackageRow> {
    Ok(PackageRow {
        id: row.get("id")?,
        slug: row.get("slug")?,
        source_url: row.get("source_url")?,
        current_sha256: row.get("current_sha256")?,
        status: row.get("status")?,
        first_seen_at: row.get("first_seen_at")?,
        installed_at: row.get("installed_at")?,
        last_ran_at: row.get("last_ran_at")?,
    })
}
fn insert(db: &rusqlite::Connection, inv: &Invocation) -> Result<()> {
    db.execute("INSERT INTO invocations(id,package_id,ts_started,ts_finished,raw_input,url,final_url,sha256,install_command_json,outcome,exit_code,error_message)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",rusqlite::params![inv.id,inv.package_id,inv.ts_started,inv.ts_finished,inv.raw_input,inv.url,inv.final_url,inv.sha256,inv.install_command_json,inv.outcome,inv.exit_code,inv.error_message])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::Store;
    #[cfg(unix)]
    #[test]
    fn home_and_database_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let base = tempfile::TempDir::new().unwrap();
        let home = base.path().join("home");
        Store::open(&home).unwrap();
        assert_eq!(mode(&home), 0o700);
        assert_eq!(mode(&home.join("sweep.db")), 0o600);
    }
}
