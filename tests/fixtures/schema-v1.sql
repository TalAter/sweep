CREATE TABLE schema_meta (version INTEGER PRIMARY KEY);
INSERT INTO schema_meta VALUES(1);
CREATE TABLE packages (
 id INTEGER PRIMARY KEY, slug TEXT NOT NULL, source_url TEXT NOT NULL UNIQUE,
 current_sha256 TEXT, status TEXT NOT NULL, first_seen_at TEXT NOT NULL,
 installed_at TEXT, last_ran_at TEXT
);
CREATE TABLE invocations (
 id TEXT PRIMARY KEY, package_id INTEGER REFERENCES packages(id),
 ts_started TEXT NOT NULL, ts_finished TEXT, raw_input TEXT NOT NULL,
 url TEXT, final_url TEXT, sha256 TEXT, install_command_json TEXT,
 outcome TEXT NOT NULL, exit_code INTEGER, error_message TEXT
);
INSERT INTO packages VALUES(7,'old-tool','https://www.example.org/install','abc123','installed','2025-01-01T00:00:00.000Z','2025-01-01T00:00:00.000Z','2025-01-03T00:00:00.000Z');
INSERT INTO packages VALUES(8,'failed-tool','https://failed.example/install',NULL,'failed','2025-01-02T00:00:00.000Z',NULL,NULL);
INSERT INTO invocations VALUES('existing-id',7,'2025-01-01T00:00:00.000Z','2025-01-01T00:00:01.000Z','curl https://www.example.org/install | sh','https://www.example.org/install','https://www.example.org/install','abc123','{}','ran',0,NULL);
