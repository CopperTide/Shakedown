from platform_lib.migrate import pending


def test_pending_files_run_in_name_order_and_once(tmp_path):
    for name in ("002_add_index.sql", "001_create.sql", "notes.txt", "010_more.sql"):
        (tmp_path / name).write_text("SELECT 1;")
    assert [p.name for p in pending(tmp_path, {"001_create"})] == ["002_add_index.sql", "010_more.sql"]
