//! One tool call, in one transaction.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;
use serde_json::Value;
use tokio_postgres::{Client, Transaction};

use super::fail::{Fail, R};
use super::retail::Retail;
use super::schema::{check_call, is_write};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub result: Option<Value>,
    pub error: Option<String>,
    /// Sorted.
    pub reads: Vec<String>,
    /// Sorted.
    pub writes: Vec<String>,
}

impl ToolOutcome {
    fn failed(msg: String) -> Self {
        ToolOutcome {
            result: None,
            error: Some(msg),
            reads: vec![],
            writes: vec![],
        }
    }
}

/// What a tool read and wrote, and the documents it changed.
struct Touched {
    reads: Vec<String>,
    writes: Vec<String>,
    /// `(table, id, doc)` for each write.
    docs: Vec<(String, String, Value)>,
}

impl Touched {
    fn of(retail: &Retail<'_>) -> Self {
        let docs = retail
            .writes
            .iter()
            .map(|key| {
                let (table, id) = key.split_once('/').expect("write keys are table/id");
                (table.to_string(), id.to_string(), retail.cache[key].clone())
            })
            .collect();
        Touched {
            reads: retail.reads.iter().cloned().collect(),
            writes: retail.writes.iter().cloned().collect(),
            docs,
        }
    }

    /// Writes every changed document back to its row.
    async fn persist(&self, tx: &Transaction<'_>) -> Result<()> {
        for (table, id, doc) in &self.docs {
            tx.execute(
                &format!("UPDATE {table} SET doc = $1 WHERE id = $2"),
                &[doc, id],
            )
            .await?;
        }
        Ok(())
    }
}

/// Runs one tool in one transaction. `Err` only for database failures; tool failures,
/// unknown tools and bad arguments come back in `ToolOutcome.error`.
pub async fn call(client: &mut Client, name: &str, args: &Value) -> Result<ToolOutcome> {
    run(client, name, args, Keep::Writes).await
}

/// `call`, answered as if its writes were made, but none is: the transaction is rolled back and
/// no key is written (a lost write).
pub async fn call_unwritten(client: &mut Client, name: &str, args: &Value) -> Result<ToolOutcome> {
    run(client, name, args, Keep::Nothing).await
}

/// Whether a successful call's writes are kept.
#[derive(Clone, Copy, PartialEq)]
enum Keep {
    Writes,
    Nothing,
}

async fn run(client: &mut Client, name: &str, args: &Value, keep: Keep) -> Result<ToolOutcome> {
    if let Err(msg) = check_call(name, args) {
        return Ok(ToolOutcome::failed(msg));
    }
    let tx = client.transaction().await?;
    let mut retail = Retail {
        tx: &tx,
        lock: is_write(name),
        cache: HashMap::new(),
        all_loaded: HashSet::new(),
        reads: BTreeSet::new(),
        writes: BTreeSet::new(),
    };
    let out = retail.run(name, args).await;
    let mut touched = Touched::of(&retail);
    if keep == Keep::Nothing {
        touched.writes.clear();
        touched.docs.clear();
    }
    finish(tx, out, touched).await
}

/// Commit the transaction when the tool succeeded, roll it back when it failed.
async fn finish(tx: Transaction<'_>, out: R<Value>, touched: Touched) -> Result<ToolOutcome> {
    let (result, error) = match out {
        Ok(result) => {
            touched.persist(&tx).await?;
            tx.commit().await?;
            (Some(result), None)
        }
        Err(Fail::Tool(msg)) => {
            // Nothing is persisted on failure (as tool.py's main); sets report what was attempted.
            tx.rollback().await?;
            (None, Some(msg))
        }
        Err(Fail::Db(e)) => return Err(e),
    };
    Ok(ToolOutcome {
        result,
        error,
        reads: touched.reads,
        writes: touched.writes,
    })
}
