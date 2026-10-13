use crate::{config::Rules, engine, models::*};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::mpsc,
};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Store {
    tx: mpsc::SyncSender<Job>,
}
impl Store {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS samples (key TEXT NOT NULL,at TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(key,at)); CREATE INDEX IF NOT EXISTS samples_at ON samples(at); CREATE TABLE IF NOT EXISTS views (key TEXT PRIMARY KEY,json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS signals (id TEXT PRIMARY KEY,token TEXT NOT NULL,rule TEXT NOT NULL,at TEXT NOT NULL,json TEXT NOT NULL); CREATE INDEX IF NOT EXISTS signals_cooldown ON signals(token,rule,at); CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY,json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS watchlist (chain TEXT NOT NULL,address TEXT NOT NULL,PRIMARY KEY(chain,address));")?;
        let (tx, rx) = mpsc::sync_channel::<Job>(64);
        std::thread::Builder::new()
            .name("sqlite-single-writer".into())
            .spawn(move || {
                let mut conn = conn;
                while let Ok(job) = rx.recv() {
                    job(&mut conn)
                }
            })?;
        Ok(Self { tx })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        // A bounded queue prevents unbounded memory; never block a Tokio worker on a full queue.
        let job: Job = Box::new(move |c| {
            let _ = tx.send(f(c));
        });
        self.tx
            .try_send(job)
            .map_err(|e| anyhow::anyhow!("database queue unavailable: {e}"))?;
        rx.await.context("database worker stopped")?
    }
    pub async fn ingest(
        &self,
        items: Vec<TokenSnapshot>,
        sources: Vec<SourceStatus>,
        errors: Vec<String>,
        rules: Rules,
    ) -> Result<usize> {
        self.call(move|c| {
            let tx=c.transaction()?;let mut dedup:BTreeMap<String,TokenSnapshot>=BTreeMap::new();
            for mut t in items {
                if !t.valid() {continue}
                t.token_address=canonical_address(&t.chain,&t.token_address);t.pool_address=canonical_address(&t.chain,&t.pool_address);
                let key=t.key();
                if let Some(existing)=dedup.get_mut(&key) {
                    let labels:Vec<_>=existing.discovery_labels.iter().chain(t.discovery_labels.iter()).cloned().collect();
                    if timestamp(&t.fetched_at)>timestamp(&existing.fetched_at) {*existing=t;}
                    existing.discovery_labels=labels;existing.discovery_labels.sort();existing.discovery_labels.dedup();
                } else {dedup.insert(key,t);}
            }
            let mut accepted=0usize;
            for (key,t) in dedup {
                let previous:Option<TokenView>=tx.query_row("SELECT json FROM views WHERE key=?1",[&key],|r|r.get::<_,String>(0)).optional()?.map(|v|serde_json::from_str(&v)).transpose()?;
                if previous.as_ref().is_some_and(|p|timestamp(&p.token.fetched_at)>=timestamp(&t.fetched_at)) {continue}
                let row=serde_json::to_string(&t)?;
                if tx.execute("INSERT OR IGNORE INTO samples(key,at,json) VALUES(?1,?2,?3)",params![key,t.fetched_at,row])?==0 {continue}
                let first=previous.as_ref().map(|v|v.first_seen_at.clone()).unwrap_or_else(||t.fetched_at.clone());
                let history={
                    let mut stmt=tx.prepare("SELECT json FROM samples WHERE key=?1 AND at>=?2 ORDER BY at")?;
                    let rows=stmt.query_map(params![key,first],|r|r.get::<_,String>(0))?;
                    let mut history=Vec::new();for row in rows {let p:TokenSnapshot=serde_json::from_str(&row?)?;history.push(Point{at:p.fetched_at,price_usd:p.price_usd,liquidity_usd:p.liquidity_usd,volume_24h_usd:p.volume_24h_usd});}
                    history
                };
                let clock=chrono::Utc::now();
                let (mut view,emitted)=engine::evaluate(t,previous.as_ref(),history,&rules,clock);
                for sig in emitted {
                    let token=format!("{}:{}",sig.chain,sig.token_address);
                    let last:Option<String>=tx.query_row("SELECT at FROM signals WHERE token=?1 AND rule=?2 ORDER BY at DESC LIMIT 1",params![token,sig.rule],|r|r.get(0)).optional()?;
                    if last.as_deref().and_then(timestamp).is_some_and(|at|(clock-at).num_seconds()<rules.cooldown_seconds) {continue}
                    tx.execute("INSERT OR IGNORE INTO signals(id,token,rule,at,json) VALUES(?1,?2,?3,?4,?5)",params![sig.id,token,sig.rule,sig.at,serde_json::to_string(&sig)?])?;
                    view.timeline.push(sig);
                }
                view.timeline=view.timeline.into_iter().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
                // Outcomes use the entire observed interval before payload size is capped.
                if view.history.len()>500 {view.history=view.history.split_off(view.history.len()-500);}
                tx.execute("INSERT INTO views(key,json) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET json=excluded.json",params![key,serde_json::to_string(&view)?])?;
                accepted+=1;
            }
            for source in &sources {
                if source.status=="SOURCE_ERROR" || source.status=="UNAVAILABLE" {
                    let token=format!("source:{}",source.id);let rule="SOURCE_DISCONNECTED";
                    let last:Option<String>=tx.query_row("SELECT at FROM signals WHERE token=?1 AND rule=?2 ORDER BY at DESC LIMIT 1",params![token,rule],|r|r.get(0)).optional()?;
                    if last.as_deref().and_then(timestamp).is_none_or(|at|(chrono::Utc::now()-at).num_seconds()>=rules.cooldown_seconds) {
                        let at=now();let sig=Signal{id:format!("{token}:{at}"),chain:"system".into(),token_address:source.id.clone(),pool_address:String::new(),rule:rule.into(),severity:"RISK".into(),at:at.clone(),state:"INVALIDATED".into(),message:format!("数据源不可用：{}",source.id),evidence:vec![source.detail.clone(),format!("last_success={:?}",source.last_success_at)]};
                        tx.execute("INSERT OR IGNORE INTO signals(id,token,rule,at,json) VALUES(?1,?2,?3,?4,?5)",params![sig.id,token,rule,at,serde_json::to_string(&sig)?])?;
                    }
                }
            }
            for (key,value) in [("sources",serde_json::to_string(&sources)?),("provider_errors",serde_json::to_string(&errors)?),("last_scan_at",serde_json::to_string(&now())?)] {tx.execute("INSERT INTO meta(key,json) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET json=excluded.json",params![key,value])?;}
            tx.commit()?;Ok(accepted)
        }).await
    }
    pub async fn snapshot(&self, mode: &str) -> Result<Snapshot> {
        let mode = mode.to_owned();
        self.call(move|c| {
            let mut snapshot=Snapshot::empty(&mode);
            let mut stmt=c.prepare("SELECT json FROM views ORDER BY key")?;
            for row in stmt.query_map([],|r|r.get::<_,String>(0))? {snapshot.items.push(serde_json::from_str(&row?)?);}
            let mut names:HashMap<(String,String),Vec<String>>=HashMap::new();
            for v in &snapshot.items {names.entry((v.token.chain.clone(),v.token.symbol.to_lowercase())).or_default().push(v.token.token_address.clone());}
            for v in &mut snapshot.items {
                if let Some(addresses)=names.get(&(v.token.chain.clone(),v.token.symbol.to_lowercase())) {if addresses.iter().any(|a|*a!=v.token.token_address) {v.incomplete_data.push("同名代币：以完整合约和池地址为准，名称不能用于身份确认".into());}}
                if timestamp(&v.token.fetched_at).is_some_and(|at|(chrono::Utc::now()-at).num_seconds()>300) {v.incomplete_data.push("STALE：该池数据超过5分钟未更新".into());}
            }
            for (key,dest) in [("sources",0),("provider_errors",1),("last_scan_at",2)] {
                if let Some(value)=c.query_row("SELECT json FROM meta WHERE key=?1",[key],|r|r.get::<_,String>(0)).optional()? {match dest {0=>snapshot.sources=serde_json::from_str(&value)?,1=>snapshot.provider_errors=serde_json::from_str(&value)?,_=>snapshot.generated_at_utc=serde_json::from_str(&value)?}}
            }
            let mut stmt=c.prepare("SELECT json FROM signals ORDER BY at DESC LIMIT 200")?;
            for row in stmt.query_map([],|r|r.get::<_,String>(0))? {snapshot.signals.push(serde_json::from_str(&row?)?);}
            let count:i64=c.query_row("SELECT count(*) FROM samples",[],|r|r.get(0))?;
            let alerts:i64=c.query_row("SELECT count(*) FROM signals",[],|r|r.get(0))?;
            snapshot.metrics=json!({"samples":count,"pools":snapshot.items.len(),"signals":alerts,"research":research(&snapshot.items),"rules_are_investment_advice":false});
            if mode=="LOCAL_LIVE" {snapshot.max_staleness_seconds=300;}
            Ok(snapshot)
        }).await
    }
    pub async fn watchlist(&self) -> Result<Vec<(String, String)>> {
        self.call(|c| {
            let mut stmt =
                c.prepare("SELECT chain,address FROM watchlist ORDER BY chain,address")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
    }
    pub async fn watch(&self, chain: &str, address: &str, remove: bool) -> Result<()> {
        anyhow::ensure!(
            valid_address(chain, address),
            "invalid chain or full token address"
        );
        let chain = chain.to_owned();
        let address = canonical_address(&chain, address);
        self.call(move |c| {
            if remove {
                c.execute(
                    "DELETE FROM watchlist WHERE chain=?1 AND address=?2",
                    params![chain, address],
                )?;
            } else {
                let exists: bool = c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM watchlist WHERE chain=?1 AND address=?2)",
                    params![chain, address],
                    |r| r.get(0),
                )?;
                if exists {
                    return Ok(());
                }
                let count: i64 = c.query_row("SELECT count(*) FROM watchlist", [], |r| r.get(0))?;
                anyhow::ensure!(count < 60, "watchlist limit: 60");
                c.execute(
                    "INSERT OR IGNORE INTO watchlist(chain,address) VALUES(?1,?2)",
                    params![chain, address],
                )?;
            }
            Ok(())
        })
        .await
    }
    pub async fn doctor(&self) -> Result<Value> {
        self.call(|c|{let integrity:String=c.query_row("PRAGMA integrity_check",[],|r|r.get(0))?;let mode:String=c.query_row("PRAGMA journal_mode",[],|r|r.get(0))?;let count:i64=c.query_row("SELECT count(*) FROM samples",[],|r|r.get(0))?;Ok(json!({"integrity_check":integrity,"journal_mode":mode,"samples":count,"checked_at":now()}))}).await
    }
    pub async fn archive(&self, output: &str, retention_days: i64) -> Result<usize> {
        anyhow::ensure!(retention_days >= 1, "retention must be at least 1 day");
        let output = output.to_owned();
        self.call(move |c| {
            let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let rows = {
                let mut stmt =
                    c.prepare("SELECT key,at,json FROM samples WHERE at<?1 ORDER BY at")?;
                let iter = stmt.query_map([&cutoff], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?;
                iter.collect::<std::result::Result<Vec<_>, _>>()?
            };
            if let Some(parent) = Path::new(&output).parent() {
                std::fs::create_dir_all(parent)?;
            }
            // create_new prevents destroying an existing backup. Sync succeeds before deleting rows.
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)?;
            file.write_all(&serde_json::to_vec(
                &json!({"schema":1,"archived_at":now(),"rows":rows}),
            )?)?;
            file.sync_all()?;
            c.execute("DELETE FROM samples WHERE at<?1", [cutoff])?;
            Ok(rows.len())
        })
        .await
    }
    pub async fn restore(&self, input: &str) -> Result<usize> {
        let v: Value = serde_json::from_slice(&std::fs::read(input)?)?;
        anyhow::ensure!(v["schema"] == 1, "unsupported archive schema");
        let rows: Vec<(String, String, String)> = serde_json::from_value(v["rows"].clone())?;
        self.call(move |c| {
            let tx = c.transaction()?;
            let mut count = 0;
            for (key, at, row) in rows {
                let token: TokenSnapshot = serde_json::from_str(&row)?;
                anyhow::ensure!(
                    token.valid() && token.key() == key && token.fetched_at == at,
                    "invalid archive row"
                );
                count += tx.execute(
                    "INSERT OR IGNORE INTO samples(key,at,json) VALUES(?1,?2,?3)",
                    params![key, at, row],
                )?;
            }
            tx.commit()?;
            Ok(count)
        })
        .await
    }
}

fn research(items: &[TokenView]) -> Value {
    // Deterministic address hash selection avoids hindsight; it is a research baseline, not a strategy claim.
    let random: Vec<_> = items
        .iter()
        .filter(|v| {
            v.token
                .token_address
                .bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
                % 5
                == 0
        })
        .collect();
    let eligible: Vec<_> = items
        .iter()
        .filter(|v| v.timeline.iter().any(|s| s.severity == "CONFIRMATION"))
        .collect();
    let all: Vec<_> = items.iter().collect();
    fn aggregate(group: &[&TokenView]) -> Value {
        let mut windows = serde_json::Map::new();
        for label in ["1h", "2h", "6h"] {
            let complete: Vec<_> = group
                .iter()
                .map(|v| &v.outcome["baseline_from_first_observation"]["windows"][label])
                .filter(|v| v["status"] == "COMPLETE")
                .collect();
            let mean = |key: &str| -> Option<f64> {
                if complete.is_empty() {
                    None
                } else {
                    Some(
                        complete.iter().filter_map(|v| v[key].as_f64()).sum::<f64>()
                            / complete.len() as f64,
                    )
                }
            };
            windows.insert(label.into(),json!({"complete_count":complete.len(),"unknown_count":group.len()-complete.len(),"mean_max_positive_pct":mean("max_positive_pct"),"mean_max_negative_pct":mean("max_negative_pct")}));
        }
        json!({"candidate_count":group.len(),"windows":windows})
    }
    json!({"method":"All groups compare SAMPLED outcomes anchored at first observation. Address-hash mod5 pseudo-random baseline fixed without price data. Confirmed cohort selected later: selection/survivorship bias; not causal or a strategy backtest.","confirmed_candidates":eligible.len(),"all_candidates_baseline":items.len(),"deterministic_random_candidates":random.len(),"completed_1h":items.iter().filter(|v|v.outcome["windows"]["1h"]["status"]=="COMPLETE").count(),"comparison":{"confirmed":aggregate(&eligible),"all":aggregate(&all),"pseudo_random":aggregate(&random)},"status":"RESEARCH_ONLY_NO_PROFITABILITY_CLAIM"})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(at: &str) -> TokenSnapshot {
        TokenSnapshot {
            chain: "solana".into(),
            token_address: "So11111111111111111111111111111111111111112".into(),
            pool_address: "pool".into(),
            fetched_at: at.into(),
            price_usd: Some(1.0),
            liquidity_usd: Some(5000.0),
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn dedup_restart_out_of_order_and_cooldown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        let path = path.to_str().unwrap();
        let db = Store::open(path).unwrap();
        let at = now();
        let a = sample(&at);
        let mut b = a.clone();
        b.source = "second".into();
        assert_eq!(
            db.ingest(vec![a.clone(), b], vec![], vec![], Rules::default())
                .await
                .unwrap(),
            1
        );
        let db2 = Store::open(path).unwrap();
        assert_eq!(
            db2.ingest(vec![a], vec![], vec![], Rules::default())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            db2.ingest(
                vec![sample("2000-01-01T00:00:00.000Z")],
                vec![],
                vec![],
                Rules::default()
            )
            .await
            .unwrap(),
            0
        );
        let later = (timestamp(&at).unwrap() + chrono::Duration::seconds(120)).to_rfc3339();
        let mut risk = sample(&later);
        risk.sells_5m = Some(99);
        risk.buys_5m = Some(1);
        db2.ingest(vec![risk.clone()], vec![], vec![], Rules::default())
            .await
            .unwrap();
        risk.fetched_at = (timestamp(&at).unwrap() + chrono::Duration::seconds(240)).to_rfc3339();
        db2.ingest(vec![risk], vec![], vec![], Rules::default())
            .await
            .unwrap();
        let snapshot = db2.snapshot("LOCAL_LIVE").await.unwrap();
        assert_eq!(
            snapshot
                .signals
                .iter()
                .filter(|s| s.rule == "SELL_COUNT_IMBALANCE")
                .count(),
            1
        );
        assert_eq!(db2.doctor().await.unwrap()["integrity_check"], "ok");
    }
    #[tokio::test]
    async fn watchlist_full_duplicate_remains_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().join("watch.db").to_str().unwrap()).unwrap();
        for i in 0..60 {
            db.watch("bsc", &format!("0x{i:040x}"), false)
                .await
                .unwrap();
        }
        db.watch("bsc", &format!("0x{:040x}", 0), false)
            .await
            .unwrap();
        assert!(db
            .watch("bsc", &format!("0x{:040x}", 60), false)
            .await
            .is_err());
        assert_eq!(db.watchlist().await.unwrap().len(), 60);
    }
    #[tokio::test]
    async fn source_failure_signals_are_cooldown_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().join("source.db").to_str().unwrap()).unwrap();
        let sources = vec![SourceStatus {
            id: "fixture_failure".into(),
            status: "SOURCE_ERROR".into(),
            checked_at: now(),
            detail: "offline test timeout".into(),
            ..Default::default()
        }];
        for _ in 0..2 {
            db.ingest(
                vec![],
                sources.clone(),
                vec!["timeout".into()],
                Rules::default(),
            )
            .await
            .unwrap();
        }
        let s = db.snapshot("LOCAL_LIVE").await.unwrap();
        assert_eq!(s.signals.len(), 1);
        assert_eq!(s.signals[0].rule, "SOURCE_DISCONNECTED");
        assert!(s.items.is_empty());
    }
    #[tokio::test]
    async fn archive_restore_idempotent_and_unknown_not_zero() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().join("test.db").to_str().unwrap()).unwrap();
        db.ingest(
            vec![sample("2000-01-01T00:00:00.000Z")],
            vec![],
            vec![],
            Rules::default(),
        )
        .await
        .unwrap();
        let out = dir.path().join("archive.json");
        let out = out.to_str().unwrap();
        assert_eq!(db.archive(out, 30).await.unwrap(), 1);
        assert_eq!(db.restore(out).await.unwrap(), 1);
        assert_eq!(db.restore(out).await.unwrap(), 0);
        assert!(db.snapshot("LOCAL_LIVE").await.unwrap().items[0]
            .token
            .market_cap_usd
            .is_none());
        assert!(db.archive(out, 30).await.is_err());
    }
}
