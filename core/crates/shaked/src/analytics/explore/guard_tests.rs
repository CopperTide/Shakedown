use super::*;

fn refused(sql: &str) -> String {
    match check(sql) {
        Err(why) => why,
        Ok(c) => panic!("{sql} passed as {}", c.sql),
    }
}

#[test]
fn a_select_on_the_views_passes_and_is_printed_back() {
    let sql = "select model, count(*) as calls, avg(think_ms) from model_calls
               where at > now() - interval '7 days' group by model order by 2 desc limit 10;";
    let c = check(sql).unwrap();
    assert!(
        c.sql.starts_with("SELECT model, count(*) AS calls"),
        "{}",
        c.sql
    );
    assert!(!c.sql.ends_with(';'));
    check("SELECT * FROM explore.runs r JOIN steps s USING (branch_id)").unwrap();
    check("WITH p AS (SELECT setup, avg(passed::int) AS rate FROM runs GROUP BY setup) SELECT * FROM p").unwrap();
    check("SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY seconds) FROM runs").unwrap();
    check(
        "SELECT date_trunc('day', started_at), count(*) FILTER (WHERE passed) FROM runs GROUP BY 1",
    )
    .unwrap();
    check("SELECT tool, row_number() OVER (PARTITION BY branch_id ORDER BY step) FROM tool_calls")
        .unwrap();
    check("SELECT run_id FROM runs WHERE branch_id IN (SELECT branch_id FROM tool_calls WHERE tool = 'x')")
        .unwrap();
    check("SELECT extract(epoch FROM ended_at - started_at), a.model FROM runs, agents a").unwrap();
    check("SELECT 1 UNION ALL SELECT 2").unwrap();
}

#[test]
fn writes_ddl_and_other_statements_are_refused() {
    for sql in [
        "INSERT INTO runs (run_id) VALUES ('x')",
        "UPDATE runs SET status = 'passed'",
        "DELETE FROM runs",
        "DROP VIEW runs",
        "CREATE TABLE t (a int)",
        "ALTER TABLE runs ADD COLUMN x int",
        "TRUNCATE runs",
        "GRANT SELECT ON runs TO public",
        "COPY runs TO '/tmp/x'",
        "COPY (SELECT 1) TO PROGRAM 'id'",
        "SET ROLE postgres",
        "SET search_path = shakedown",
        "RESET ROLE",
        "EXPLAIN ANALYZE SELECT 1",
        "BEGIN",
        "COMMIT",
        "VACUUM",
        "LISTEN x",
        "SHOW search_path",
        "DO $$ BEGIN END $$",
        "CALL p()",
        "PREPARE p AS SELECT 1",
    ] {
        refused(sql);
    }
}

#[test]
fn more_than_one_statement_is_refused() {
    assert_eq!(refused("SELECT 1; DELETE FROM runs"), "one statement only");
    assert_eq!(refused("SELECT 1; SELECT 2"), "one statement only");
    refused("");
}

#[test]
fn writes_hidden_in_a_query_are_refused() {
    refused("WITH d AS (DELETE FROM runs RETURNING *) SELECT * FROM d");
    refused("WITH u AS (UPDATE runs SET status = 'x' RETURNING *) SELECT 1");
    refused("WITH i AS (INSERT INTO runs VALUES (1) RETURNING *) SELECT * FROM i");
    refused("SELECT * INTO copy_of_runs FROM runs");
    refused("SELECT * FROM runs FOR UPDATE");
    refused("SELECT * FROM runs FOR SHARE");
    refused("TABLE runs");
}

#[test]
fn internal_tables_are_unreachable_by_any_name() {
    for sql in [
        "SELECT * FROM provider_keys",
        "SELECT * FROM shakedown.provider_keys",
        "SELECT * FROM \"shakedown\".\"provider_keys\"",
        "SELECT * FROM shakedown.events",
        "SELECT * FROM shakedown.api_tokens",
        "SELECT * FROM shakedown.run_tokens",
        "SELECT * FROM shakedown.connections",
        "SELECT * FROM shakedown.observability_targets",
        "SELECT * FROM shakedown_hosted.sandboxes",
        "SELECT * FROM identity.users",
        "SELECT * FROM identity.audit_log",
        "SELECT * FROM pg_authid",
        "SELECT * FROM pg_catalog.pg_authid",
        "SELECT * FROM pg_shadow",
        "SELECT * FROM information_schema.tables",
        "SELECT * FROM mydb.explore.runs",
        "SELECT * FROM public.runs",
        "SELECT * FROM \"Runs\"",
        "SELECT * FROM runs WHERE run_id IN (SELECT branch_id FROM shakedown.events)",
        "SELECT (SELECT sealed_key FROM shakedown.provider_keys LIMIT 1) FROM runs",
        "SELECT * FROM runs JOIN shakedown.saves s ON true",
        "SELECT * FROM runs WHERE EXISTS (SELECT 1 FROM pg_catalog.pg_user)",
        "SELECT * FROM (SELECT * FROM shakedown.reviews) x",
        "SELECT * FROM runs UNION SELECT * FROM shakedown.runs",
        "WITH x AS (SELECT * FROM shakedown.provider_keys) SELECT * FROM x",
    ] {
        refused(sql);
    }
}

#[test]
fn a_cte_may_not_shadow_the_catalog() {
    refused("WITH pg_authid AS (SELECT 1) SELECT * FROM pg_authid");
    check("WITH mine AS (SELECT 1 AS a) SELECT * FROM mine").unwrap();
}

#[test]
fn functions_outside_the_allowlist_are_refused() {
    for sql in [
        "SELECT pg_read_file('/etc/passwd')",
        "SELECT pg_sleep(100)",
        "SELECT current_setting('app.caller')",
        "SELECT set_config('role', 'postgres', true)",
        "SELECT query_to_xml('select * from shakedown.provider_keys', true, true, '')",
        "SELECT dblink('host=x', 'select 1')",
        "SELECT lo_import('/etc/passwd')",
        "SELECT pg_catalog.count(*) FROM runs",
        "SELECT shakedown.step_tool('tool_call', '{}')",
        "SELECT txid_current()",
        "SELECT version()",
        "SELECT pg_terminate_backend(1)",
        "SELECT * FROM generate_series(1, 10)",
        "SELECT * FROM pg_ls_dir('.')",
        "SELECT * FROM runs, LATERAL pg_read_file('/x') f",
        "SELECT * FROM runs WHERE pg_sleep(1) IS NULL",
        "SELECT count(*) FILTER (WHERE pg_sleep(1) IS NULL) FROM runs",
        "SELECT \"pg_sleep\"(1)",
    ] {
        refused(sql);
    }
}

#[test]
fn a_long_query_is_refused_before_parsing() {
    let long = format!("SELECT 1 {}", " ".repeat(MAX_SQL_BYTES));
    assert!(refused(&long).contains("at most"));
}

#[test]
fn deep_nesting_is_refused_not_a_crash() {
    let deep = format!("SELECT {}1{}", "(".repeat(5000), ")".repeat(5000));
    refused(&deep);
}
